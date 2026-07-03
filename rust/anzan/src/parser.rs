//! Pratt (precedence-climbing) parser.
//!
//! Grammar, low to high precedence:
//!   assignment:  IDENT = expr          (only at top level)
//!   additive:    + -
//!   term:        * / %  and implicit multiplication (`2(3+4)`, `2x`, `(a)(b)`)
//!   unary:       -x
//!   power:       ^ (right-associative, binds tighter than unary: -2^2 == -4)
//!   primary:     number | ident | ident(args) | (expr)

use crate::ast::{
    ComparisonOperator, Expression, MapLiteralEntry, Parameter, ReductionOperation, TypeAnnotation,
};
use crate::eval::data_type::{DataField, DataFieldType};
use crate::lexer::{Lexer, Token, TokenKind};
use crate::{BigDecimal, EngineError, LanguageMode};

/// Identifiers that cannot be assigned to (or defined as functions).
/// `sigma` is the special summation form, so a user function would be
/// uncallable anyway.
pub(crate) const RESERVED_NAMES: [&str; 15] = [
    "ans", "pi", "e", "tau", "π", "τ", "sigma", "if", "man", "manual", "help", "true", "false",
    "json", "rounding",
];

pub(crate) fn is_reserved(name: &str) -> bool {
    RESERVED_NAMES.contains(&name.to_lowercase().as_str())
}

pub struct Parser {
    tokens: Vec<Token>,
    /// The dialect to parse under — affects only overloaded glyphs
    /// (`^ % & | << >>`). `Normal` (the default) is today's grammar exactly.
    /// See `docs/MODES.md`.
    mode: LanguageMode,
    index: usize,
}

fn parse_error(message: impl Into<String>, position: usize) -> EngineError {
    EngineError::ParseError {
        message: message.into(),
        position,
    }
}

impl Parser {
    pub fn parse(source: &str, mode: LanguageMode) -> Result<Expression, EngineError> {
        let mut parser = Parser {
            tokens: Lexer::tokenize(source)?,
            mode,
            index: 0,
        };
        let expr = parser.statement()?;
        parser.expect_end()?;
        Ok(expr)
    }

    /// `sigma_x`/`product_x` spellings are reserved for the indexed
    /// reduction forms.
    pub(crate) fn is_reduction_name(name: &str) -> bool {
        let lowered = name.to_lowercase();
        lowered.starts_with("sigma_") || lowered.starts_with("product_")
    }

    fn current(&self) -> &Token {
        &self.tokens[self.index]
    }

    fn kind_at(&self, i: usize) -> Option<&TokenKind> {
        self.tokens.get(i).map(|t| &t.kind)
    }

    fn advance(&mut self) -> Token {
        let token = self.tokens[self.index].clone();
        if self.index < self.tokens.len() - 1 {
            self.index += 1;
        }
        token
    }

    fn expect_end(&self) -> Result<(), EngineError> {
        if self.current().kind == TokenKind::End {
            Ok(())
        } else {
            Err(parse_error(
                "unexpected trailing input",
                self.current().position(),
            ))
        }
    }

    // MARK: Productions

    /// `x = expr`, `f(a, b) = expr`, or a plain expression.
    fn statement(&mut self) -> Result<Expression, EngineError> {
        if let TokenKind::Identifier(name) = &self.current().kind {
            // sigma_i=1^10(…) is a summation, not an assignment.
            if self.kind_at(self.index + 1) == Some(&TokenKind::Assign)
                && !Self::is_reduction_name(name)
            {
                let name = name.clone();
                let position = self.current().position();
                if is_reserved(&name) {
                    return Err(parse_error(format!("cannot assign to '{name}'"), position));
                }
                self.index += 2;
                return Ok(Expression::Assignment {
                    name,
                    value: Box::new(self.comparison()?),
                });
            }
            // `import Bits` — contextual: only `import` followed by a name
            // commits, so `import` stays usable as a variable and
            // `import = 5` is an assignment.
            if name.eq_ignore_ascii_case("import") {
                if let Some(TokenKind::Identifier(namespace)) = self.kind_at(self.index + 1) {
                    let namespace = namespace.clone();
                    self.index += 2;
                    return Ok(Expression::ImportDirective { name: namespace });
                }
            }
        }
        if let Some(namespace_definition) = self.namespace_definition()? {
            return Ok(namespace_definition);
        }
        if let Some(data_definition) = self.data_definition()? {
            return Ok(data_definition);
        }
        if let Some(definition) = self.function_definition()? {
            return Ok(definition);
        }
        self.comparison()
    }

    /// `namespace Bits { data BitField { … }  data BitFormat { … } }`. Like
    /// `data`, a CONTEXTUAL keyword — committed only by `namespace Ident {`.
    /// The body holds `data` declarations (docs/MODULES.md); other members
    /// are rejected by the evaluator with a clear message.
    fn namespace_definition(&mut self) -> Result<Option<Expression>, EngineError> {
        let TokenKind::Identifier(keyword) = &self.current().kind else {
            return Ok(None);
        };
        if !keyword.eq_ignore_ascii_case("namespace") || self.index + 2 >= self.tokens.len() {
            return Ok(None);
        }
        let Some(TokenKind::Identifier(name)) = self.kind_at(self.index + 1) else {
            return Ok(None);
        };
        if self.kind_at(self.index + 2) != Some(&TokenKind::LeftBrace) {
            return Ok(None);
        }
        let name = name.clone();
        let name_position = self.tokens[self.index + 1].position();
        self.index += 3; // past `namespace Name {`

        if !name.chars().next().is_some_and(char::is_uppercase) {
            return Err(parse_error(
                "namespace names start with a capital letter — e.g. namespace Bits { … }",
                name_position,
            ));
        }
        if is_reserved(&name) || Self::is_reduction_name(&name) {
            return Err(parse_error(
                format!("cannot define '{name}'"),
                name_position,
            ));
        }

        // Members are `;`-separated (a function body would otherwise run into
        // the next member via implicit multiplication); a trailing `;` is
        // fine.
        let mut members: Vec<Expression> = Vec::new();
        loop {
            if self.current().kind == TokenKind::RightBrace {
                break;
            }
            if self.current().kind == TokenKind::End {
                return Err(parse_error(
                    format!("expected '}}' to close namespace {name}"),
                    self.current().position(),
                ));
            }
            members.push(self.statement()?);
            match self.current().kind {
                TokenKind::Semicolon => {
                    self.advance();
                }
                TokenKind::RightBrace => {}
                _ => {
                    return Err(parse_error(
                        "separate namespace declarations with ';' — e.g. data A { … }; f(x) = …",
                        self.current().position(),
                    ));
                }
            }
        }
        self.advance(); // '}'
        if members.is_empty() {
            return Err(parse_error(
                format!(
                    "a namespace needs at least one declaration — e.g. namespace {name} {{ data Point {{ x: Number }} }}"
                ),
                name_position,
            ));
        }
        Ok(Some(Expression::NamespaceDefinition { name, members }))
    }

    /// `data Person { name: String, age: Number, active: Boolean }`.
    /// `data` is a CONTEXTUAL keyword: only the exact shape `data Ident {`
    /// commits (returns `None` otherwise), so `data = 5` stays an assignment
    /// and `data` stays a usable variable name. Matched case-insensitively,
    /// like function names.
    fn data_definition(&mut self) -> Result<Option<Expression>, EngineError> {
        let TokenKind::Identifier(keyword) = &self.current().kind else {
            return Ok(None);
        };
        if !keyword.eq_ignore_ascii_case("data") || self.index + 2 >= self.tokens.len() {
            return Ok(None);
        }
        let Some(TokenKind::Identifier(name)) = self.kind_at(self.index + 1) else {
            return Ok(None);
        };
        if self.kind_at(self.index + 2) != Some(&TokenKind::LeftBrace) {
            return Ok(None);
        }
        let name = name.clone();
        let name_position = self.tokens[self.index + 1].position();
        self.index += 3;

        // Definitely a declaration now — validate and parse the fields.
        if !name.chars().next().is_some_and(char::is_uppercase) {
            return Err(parse_error(
                "data type names start with a capital letter — e.g. data Person { … }",
                name_position,
            ));
        }
        if is_reserved(&name) || Self::is_reduction_name(&name) {
            return Err(parse_error(
                format!("cannot define '{name}'"),
                name_position,
            ));
        }

        let mut fields: Vec<DataField> = Vec::new();
        loop {
            match &self.current().kind {
                TokenKind::RightBrace if fields.is_empty() => {
                    return Err(parse_error(
                        format!(
                            "a data type needs at least one field — e.g. data {name} {{ name: String }}"
                        ),
                        self.current().position(),
                    ));
                }
                TokenKind::Identifier(field_name) => {
                    let field_name = field_name.clone();
                    let field_position = self.current().position();
                    self.advance();
                    if self.current().kind != TokenKind::Colon {
                        return Err(parse_error(
                            format!(
                                "expected ':' after field '{field_name}' — e.g. {field_name}: Number"
                            ),
                            self.current().position(),
                        ));
                    }
                    self.advance(); // consume ':'
                    let field_type = self.parse_field_type(&field_name)?;
                    if fields
                        .iter()
                        .any(|f| f.name.eq_ignore_ascii_case(&field_name))
                    {
                        return Err(parse_error(
                            format!("duplicate field '{field_name}'"),
                            field_position,
                        ));
                    }
                    fields.push(DataField::new(field_name, field_type));
                }
                _ => {
                    return Err(parse_error(
                        format!(
                            "expected a field — e.g. data {name} {{ name: String, age: Number }}"
                        ),
                        self.current().position(),
                    ));
                }
            }

            match self.current().kind {
                TokenKind::Comma => {
                    self.advance();
                }
                TokenKind::RightBrace => break,
                _ => {
                    return Err(parse_error(
                        "expected '}' or ','",
                        self.current().position(),
                    ));
                }
            }
        }
        self.advance(); // '}'
        Ok(Some(Expression::DataDefinition { name, fields }))
    }

    /// A field type: a leaf (Number/String/Boolean/a data type), a list
    /// `[T]`, or a string-keyed map `{String: T}` — recursive, so
    /// `[[Number]]` and `{String: [Point]}` work.
    fn parse_field_type(&mut self, field_name: &str) -> Result<DataFieldType, EngineError> {
        match &self.current().kind {
            TokenKind::LeftBracket => {
                self.advance();
                let element = self.parse_field_type(field_name)?;
                if self.current().kind != TokenKind::RightBracket {
                    return Err(parse_error(
                        "expected ']' to close the list type — e.g. [String]",
                        self.current().position(),
                    ));
                }
                self.advance();
                return Ok(DataFieldType::List(Box::new(element)));
            }
            TokenKind::LeftBrace => {
                self.advance();
                let is_string_key = matches!(
                    &self.current().kind,
                    TokenKind::Identifier(key) if key.eq_ignore_ascii_case("string")
                );
                if !is_string_key {
                    return Err(parse_error(
                        "map field keys are String — e.g. {String: Number}",
                        self.current().position(),
                    ));
                }
                self.advance();
                if self.current().kind != TokenKind::Colon {
                    return Err(parse_error(
                        "expected ':' in the map type — e.g. {String: Number}",
                        self.current().position(),
                    ));
                }
                self.advance();
                let value_type = self.parse_field_type(field_name)?;
                if self.current().kind != TokenKind::RightBrace {
                    return Err(parse_error(
                        "expected '}' to close the map type — e.g. {String: Number}",
                        self.current().position(),
                    ));
                }
                self.advance();
                return Ok(DataFieldType::Map(Box::new(value_type)));
            }
            TokenKind::Identifier(type_name) => {
                if let Some(field_type) = DataFieldType::parsing(type_name) {
                    self.advance();
                    return Ok(field_type);
                }
            }
            _ => {}
        }
        Err(parse_error(
            format!(
                "field types are Number, String, Boolean, a declared data type, or a list/map \
                 of those ([T], {{String: T}}) — e.g. {field_name}: Number"
            ),
            self.current().position(),
        ))
    }

    /// One comparison level: `additive (op additive)?`. Single comparison
    /// only — `a < b < c` is rejected (1/0 chaining is never what you meant).
    /// Lambdas are checked first: every expression entry point comes through
    /// here, so `x -> …` works as an argument, an assignment value, a body…
    fn comparison(&mut self) -> Result<Expression, EngineError> {
        if let Some(lambda) = self.lambda_expression()? {
            return Ok(lambda);
        }
        let lhs = self.bitwise_or()?;
        let Some(op) = comparison_operator(&self.current().kind) else {
            return Ok(lhs);
        };
        self.advance();
        let rhs = self.bitwise_or()?;
        if comparison_operator(&self.current().kind).is_some() {
            return Err(parse_error(
                "comparisons can't be chained — use and(a < b, b < c)",
                self.current().position(),
            ));
        }
        Ok(Expression::Comparison(op, Box::new(lhs), Box::new(rhs)))
    }

    /// Speculatively parses `x -> body` or `(a, b) -> body` (also
    /// `() -> body`). Returns `None` — position rewound — when the shape
    /// doesn't end in `->`, so `(a, b)` stays a parenthesized expression and
    /// `x` stays a variable.
    fn lambda_expression(&mut self) -> Result<Option<Expression>, EngineError> {
        let start = self.index;

        // x -> body
        if let TokenKind::Identifier(name) = &self.current().kind {
            if self.kind_at(self.index + 1) == Some(&TokenKind::Arrow) {
                let name = name.clone();
                let position = self.current().position();
                self.index += 2;
                return Ok(Some(self.lambda_body(vec![name], position)?));
            }
        }

        // (a, b) -> body — only a lambda if the parens hold a plain
        // parameter list AND an arrow follows.
        if self.current().kind != TokenKind::LeftParen {
            return Ok(None);
        }
        let position = self.current().position();
        self.advance();
        let mut parameters: Vec<String> = Vec::new();
        loop {
            match &self.current().kind {
                TokenKind::RightParen if parameters.is_empty() => {
                    // () -> …
                    self.advance();
                    break;
                }
                TokenKind::Identifier(parameter) => {
                    parameters.push(parameter.clone());
                    self.advance();
                    if self.current().kind == TokenKind::Comma {
                        self.advance();
                        continue;
                    }
                    if self.current().kind != TokenKind::RightParen {
                        self.index = start;
                        return Ok(None);
                    }
                    self.advance();
                    break;
                }
                _ => {
                    self.index = start;
                    return Ok(None);
                }
            }
        }
        if self.current().kind != TokenKind::Arrow {
            self.index = start;
            return Ok(None);
        }
        self.advance();
        Ok(Some(self.lambda_body(parameters, position)?))
    }

    fn lambda_body(
        &mut self,
        parameters: Vec<String>,
        position: usize,
    ) -> Result<Expression, EngineError> {
        let lowered: std::collections::HashSet<String> =
            parameters.iter().map(|p| p.to_lowercase()).collect();
        if lowered.len() != parameters.len() {
            return Err(parse_error("duplicate parameter name", position));
        }
        for parameter in &parameters {
            if is_reserved(parameter) {
                return Err(parse_error(
                    format!("cannot use '{parameter}' as a parameter"),
                    position,
                ));
            }
        }
        Ok(Expression::Lambda {
            parameters,
            body: Box::new(self.comparison()?),
        })
    }

    /// Speculatively parses `ident(p1, p2, …) = expr`. Returns `None` — with
    /// the position rewound — when the lookahead isn't exactly that shape, so
    /// `f(x)` stays a call and `f(2) = 1` stays a parse error downstream.
    fn function_definition(&mut self) -> Result<Option<Expression>, EngineError> {
        let start = self.index;
        // The name is an identifier (`f`, `dist`) or an arithmetic operator
        // symbol (`+`), which overloads that operator for typed operands.
        let name: String = if let TokenKind::Identifier(identifier) = &self.current().kind {
            identifier.clone()
        } else if let Some(op) = operator_definition_name(&self.current().kind) {
            op.to_string()
        } else {
            return Ok(None);
        };
        if self.kind_at(self.index + 1) != Some(&TokenKind::LeftParen) {
            return Ok(None);
        }
        let name_position = self.current().position();
        self.index += 2;

        let mut parameters: Vec<Parameter> = Vec::new();
        loop {
            match &self.current().kind {
                TokenKind::RightParen if parameters.is_empty() => {
                    // f() = …
                    self.advance();
                    break;
                }
                TokenKind::Identifier(parameter) => {
                    let parameter = parameter.clone();
                    self.advance();
                    // Optional `: Type` annotation — `dist(p: Point)`.
                    let mut annotation: Option<TypeAnnotation> = None;
                    if self.current().kind == TokenKind::Colon {
                        self.advance();
                        let TokenKind::Identifier(type_name) = &self.current().kind else {
                            self.index = start;
                            return Ok(None);
                        };
                        annotation = Some(TypeAnnotation::parsing(type_name));
                        self.advance();
                    }
                    parameters.push(Parameter {
                        name: parameter,
                        annotation,
                    });
                    if self.current().kind == TokenKind::Comma {
                        self.advance();
                        continue;
                    }
                    if self.current().kind != TokenKind::RightParen {
                        self.index = start;
                        return Ok(None);
                    }
                    self.advance();
                    break;
                }
                _ => {
                    self.index = start;
                    return Ok(None);
                }
            }
        }

        if self.current().kind != TokenKind::Assign {
            self.index = start;
            return Ok(None);
        }
        self.advance();

        // Definitely a definition now — validate and parse the body.
        if is_reserved(&name) || Self::is_reduction_name(&name) {
            return Err(parse_error(
                format!("cannot define '{name}'"),
                name_position,
            ));
        }
        let param_names: std::collections::HashSet<String> =
            parameters.iter().map(|p| p.name.to_lowercase()).collect();
        if param_names.len() != parameters.len() {
            return Err(parse_error("duplicate parameter name", name_position));
        }
        Ok(Some(Expression::FunctionDefinition {
            name,
            parameters,
            body: Box::new(self.comparison()?),
        }))
    }

    // MARK: Programmer-mode bitwise band (Python precedence)
    //
    // Loosest-to-tightest: `|` · `^` · `&` · `<< >>`, sitting between
    // comparison and additive (so bitwise binds below arithmetic, above
    // comparison — no C-style `a & b == c` trap). Active only in
    // `Programmer`; in other modes these are pass-throughs, and the glyphs
    // that have no other meaning (`| & << >>`) raise a mode-scoped error
    // rather than a vague "unexpected input". `^` is NOT errored here: in
    // `Normal`/`Finance` it is power, consumed deeper by power().

    fn bitwise_or(&mut self) -> Result<Expression, EngineError> {
        let mut lhs = self.bitwise_xor()?;
        while self.current().kind == TokenKind::Pipe {
            if self.mode != LanguageMode::Programmer {
                return Err(self.mode_operator_error("|", "bitOr", self.current().position()));
            }
            self.advance();
            lhs = Expression::Call {
                name: "bitOr".to_string(),
                arguments: vec![lhs, self.bitwise_xor()?],
            };
        }
        Ok(lhs)
    }

    fn bitwise_xor(&mut self) -> Result<Expression, EngineError> {
        let mut lhs = self.bitwise_and()?;
        while self.mode == LanguageMode::Programmer && self.current().kind == TokenKind::Caret {
            self.advance();
            lhs = Expression::Call {
                name: "bitXor".to_string(),
                arguments: vec![lhs, self.bitwise_and()?],
            };
        }
        Ok(lhs)
    }

    fn bitwise_and(&mut self) -> Result<Expression, EngineError> {
        let mut lhs = self.shift()?;
        while self.current().kind == TokenKind::Ampersand {
            if self.mode != LanguageMode::Programmer {
                return Err(self.mode_operator_error("&", "bitAnd", self.current().position()));
            }
            self.advance();
            lhs = Expression::Call {
                name: "bitAnd".to_string(),
                arguments: vec![lhs, self.shift()?],
            };
        }
        Ok(lhs)
    }

    fn shift(&mut self) -> Result<Expression, EngineError> {
        let mut lhs = self.additive()?;
        loop {
            match self.current().kind {
                TokenKind::ShiftLeft => {
                    if self.mode != LanguageMode::Programmer {
                        return Err(self.mode_operator_error(
                            "<<",
                            "bitShift",
                            self.current().position(),
                        ));
                    }
                    self.advance();
                    lhs = Expression::Call {
                        name: "bitShift".to_string(),
                        arguments: vec![lhs, self.additive()?],
                    };
                }
                TokenKind::ShiftRight => {
                    if self.mode != LanguageMode::Programmer {
                        return Err(self.mode_operator_error(
                            ">>",
                            "bitShift",
                            self.current().position(),
                        ));
                    }
                    self.advance();
                    // `a >> n` ≡ bitShift(a, -n) — bitShift shifts right on a
                    // negative count.
                    lhs = Expression::Call {
                        name: "bitShift".to_string(),
                        arguments: vec![lhs, Expression::UnaryMinus(Box::new(self.additive()?))],
                    };
                }
                _ => return Ok(lhs),
            }
        }
    }

    fn mode_operator_error(&self, glyph: &str, function: &str, position: usize) -> EngineError {
        parse_error(
            format!(
                "'{glyph}' is a Programmer-mode operator — use {function}(…), or switch to Programmer mode"
            ),
            position,
        )
    }

    fn additive(&mut self) -> Result<Expression, EngineError> {
        let mut lhs = self.term()?;
        loop {
            match self.current().kind {
                TokenKind::Plus => {
                    self.advance();
                    lhs = Expression::Binary(
                        crate::ast::BinaryOperator::Add,
                        Box::new(lhs),
                        Box::new(self.term()?),
                    );
                }
                TokenKind::Minus => {
                    self.advance();
                    lhs = Expression::Binary(
                        crate::ast::BinaryOperator::Subtract,
                        Box::new(lhs),
                        Box::new(self.term()?),
                    );
                }
                _ => return Ok(lhs),
            }
        }
    }

    fn term(&mut self) -> Result<Expression, EngineError> {
        use crate::ast::BinaryOperator::{Divide, Multiply};
        let mut lhs = self.unary()?;
        loop {
            match &self.current().kind {
                TokenKind::Star => {
                    self.advance();
                    lhs = Expression::Binary(Multiply, Box::new(lhs), Box::new(self.unary()?));
                }
                TokenKind::Slash => {
                    self.advance();
                    lhs = Expression::Binary(Divide, Box::new(lhs), Box::new(self.unary()?));
                }
                TokenKind::Percent if self.mode == LanguageMode::Programmer => {
                    // Programmer mode: `%` is modulo (mod(a, b)), at
                    // multiplicative precedence. In other modes `%` is
                    // postfix percent (postfix()).
                    self.advance();
                    lhs = Expression::Call {
                        name: "mod".to_string(),
                        arguments: vec![lhs, self.unary()?],
                    };
                }
                TokenKind::LeftParen
                | TokenKind::Identifier(_)
                | TokenKind::CellReference { .. } => {
                    // Implicit multiplication: `2(3+4)`, `2x`, `(a)(b)`,
                    // `2 A:1` — a value against a name, paren, or cell.
                    lhs = Expression::Binary(Multiply, Box::new(lhs), Box::new(self.unary()?));
                }
                TokenKind::Number(_) => {
                    // A number directly following another value (`3 4`,
                    // `3 % 4`) is almost always a missing operator, not
                    // implicit ×. Error toward it instead of silently
                    // multiplying.
                    return Err(parse_error(
                        "a number can't directly follow another value — add an operator (e.g. 3 * 4)",
                        self.current().position(),
                    ));
                }
                _ => return Ok(lhs),
            }
        }
    }

    fn unary(&mut self) -> Result<Expression, EngineError> {
        match self.current().kind {
            TokenKind::Minus => {
                self.advance();
                Ok(Expression::UnaryMinus(Box::new(self.unary()?)))
            }
            TokenKind::Plus => {
                // Unary plus is a no-op.
                self.advance();
                self.unary()
            }
            TokenKind::SqrtSign => {
                // √x desugars to sqrt(x).
                self.advance();
                Ok(Expression::Call {
                    name: "sqrt".to_string(),
                    arguments: vec![self.unary()?],
                })
            }
            TokenKind::Tilde => {
                // ~x is bitwise NOT (Programmer mode only).
                if self.mode != LanguageMode::Programmer {
                    return Err(self.mode_operator_error("~", "bitNot", self.current().position()));
                }
                self.advance();
                Ok(Expression::Call {
                    name: "bitNot".to_string(),
                    arguments: vec![self.unary()?],
                })
            }
            _ => self.power(),
        }
    }

    fn power(&mut self) -> Result<Expression, EngineError> {
        let base = self.postfix()?;
        // In Programmer mode `^` is XOR (consumed up the chain by
        // bitwise_xor); only in Normal/Finance is it power.
        if self.mode == LanguageMode::Programmer || self.current().kind != TokenKind::Caret {
            return Ok(base);
        }
        self.advance();
        // Right-associative; the exponent may carry its own unary minus
        // (2^-1).
        Ok(Expression::Binary(
            crate::ast::BinaryOperator::Power,
            Box::new(base),
            Box::new(self.unary()?),
        ))
    }

    /// Postfix accessors, binding tighter than `^`: `arr[0]`, `m.name`,
    /// chained freely (`people[0].age`, `grid[1][2]`).
    fn postfix(&mut self) -> Result<Expression, EngineError> {
        let mut expr = self.primary()?;
        loop {
            match self.current().kind {
                TokenKind::LeftBracket => {
                    self.advance();
                    let index_expr = self.comparison()?;
                    if self.current().kind != TokenKind::RightBracket {
                        return Err(parse_error("expected ']'", self.current().position()));
                    }
                    self.advance();
                    expr = Expression::Index {
                        base: Box::new(expr),
                        index: Box::new(index_expr),
                    };
                }
                TokenKind::Dot => {
                    self.advance();
                    let TokenKind::Identifier(name) = &self.current().kind else {
                        return Err(parse_error(
                            "expected a key name after '.' — e.g. person.age",
                            self.current().position(),
                        ));
                    };
                    let name = name.clone();
                    self.advance();
                    if self.current().kind == TokenKind::LeftParen {
                        // Method call: base.name(args) — hosts dispatch these.
                        self.advance();
                        let arguments = self.argument_list()?;
                        expr = Expression::MethodCall {
                            base: Box::new(expr),
                            name,
                            arguments,
                        };
                    } else {
                        expr = Expression::Member {
                            base: Box::new(expr),
                            name,
                        };
                    }
                }
                TokenKind::Percent if self.mode != LanguageMode::Programmer => {
                    // Postfix percent: `3%` → 0.03. In Normal/Finance `%` is
                    // always percent; chains like other postfixes (`A:1%`,
                    // `arr[0]%`). In Programmer mode `%` is modulo — left for
                    // term() to consume.
                    self.advance();
                    expr = Expression::Percent(Box::new(expr));
                }
                _ => break,
            }
        }
        Ok(expr)
    }

    fn primary(&mut self) -> Result<Expression, EngineError> {
        let token = self.advance();
        let token_position = token.position();
        match token.kind {
            TokenKind::Number(value) => Ok(Expression::Number(value)),

            TokenKind::CellReference { column, row, .. } => {
                self.cell_reference_or_range(None, column, row)
            }

            TokenKind::QuotedName(quoted) => {
                // 'Q1 Budget'!A:1 — a sheet qualifier when ! follows;
                // otherwise 'Projected Rate' — a NAMED CELL on the owning
                // sheet.
                if self.current().kind == TokenKind::Bang {
                    return self.qualified_reference(quoted, token_position);
                }
                Ok(Expression::NameReference {
                    sheet: None,
                    name: quoted,
                })
            }

            TokenKind::Identifier(name) if self.current().kind == TokenKind::Bang => {
                // Budget!A:1 — unquoted sheet qualifier.
                self.qualified_reference(name, token_position)
            }

            TokenKind::Identifier(name) if self.current().kind == TokenKind::ColonColon => {
                // Bits::BitFormat — namespace-qualified reference /
                // constructor call.
                self.qualified_name(name, token_position)
            }

            TokenKind::Identifier(name) => {
                // ∑/∏: a plain call is the variadic function (sum/product);
                // the subscript form is the indexed reduction. Typed
                // `sigma_i` arrives as one identifier; after the ∑/∏ symbol,
                // `_i` arrives separately.
                let lowered = name.to_lowercase();
                for (keyword, operation) in [
                    ("sigma", ReductionOperation::Sum),
                    ("product", ReductionOperation::Product),
                ] {
                    if lowered == keyword {
                        if let TokenKind::Identifier(subscript) = &self.current().kind {
                            if let Some(index_name) = subscript.strip_prefix('_') {
                                let index_name = index_name.to_string();
                                self.advance();
                                return self.math_reduction(operation, index_name, token_position);
                            }
                        }
                    } else if lowered.starts_with(&format!("{keyword}_")) {
                        let index_name = name[keyword.len() + 1..].to_string();
                        return self.math_reduction(operation, index_name, token_position);
                    }
                }

                // man NAME / manual NAME / help NAME: unix-style — the
                // argument is a NAME (never evaluated), space-separated, NO
                // parentheses.
                if lowered == "man" || lowered == "manual" || lowered == "help" {
                    if self.current().kind == TokenKind::LeftParen {
                        return Err(parse_error(
                            format!("use `{name} name` — e.g. {name} pmt (no parentheses)"),
                            self.current().position(),
                        ));
                    }
                    let TokenKind::Identifier(subject) = &self.current().kind else {
                        return Err(parse_error(
                            format!("{name} needs a function name — e.g. {name} pmt"),
                            self.current().position(),
                        ));
                    };
                    let subject = subject.clone();
                    self.advance();
                    return Ok(Expression::HelpRequest { name: subject });
                }

                if self.current().kind != TokenKind::LeftParen {
                    return Ok(Expression::Variable(name));
                }
                self.advance();
                let arguments = self.argument_list()?;
                if lowered == "sigma" {
                    // ∑(1,2,3) = 6
                    return Ok(Expression::Call {
                        name: "sum".to_string(),
                        arguments,
                    });
                }
                if lowered == "if" {
                    // Special form: branches stay lazy (the untaken one may
                    // divide by zero or recurse).
                    if arguments.len() != 3 {
                        return Err(parse_error(
                            "if expects (condition, then, else)",
                            token_position,
                        ));
                    }
                    let mut args = arguments.into_iter();
                    return Ok(Expression::Conditional {
                        condition: Box::new(args.next().expect("checked len")),
                        then: Box::new(args.next().expect("checked len")),
                        otherwise: Box::new(args.next().expect("checked len")),
                    });
                }
                // ∏(…) hits product().
                Ok(Expression::Call { name, arguments })
            }

            TokenKind::LeftParen => {
                let inner = self.comparison()?;
                if self.current().kind != TokenKind::RightParen {
                    return Err(parse_error("expected ')'", self.current().position()));
                }
                self.advance();
                Ok(inner)
            }

            TokenKind::String(text) => Ok(Expression::StringLiteral(text)),

            TokenKind::LeftBracket => self.array_literal(),

            TokenKind::LeftBrace => self.map_literal(token_position),

            TokenKind::End => Err(parse_error("unexpected end of expression", token_position)),

            _ => Err(parse_error("unexpected token", token_position)),
        }
    }

    /// `[1, 2, 3]` after the consumed '[' — elements are full expressions.
    fn array_literal(&mut self) -> Result<Expression, EngineError> {
        if self.current().kind == TokenKind::RightBracket {
            self.advance();
            return Ok(Expression::ArrayLiteral(Vec::new()));
        }
        let mut items = vec![self.comparison()?];
        while self.current().kind == TokenKind::Comma {
            self.advance();
            items.push(self.comparison()?);
        }
        if self.current().kind != TokenKind::RightBracket {
            return Err(parse_error(
                "expected ']' or ','",
                self.current().position(),
            ));
        }
        self.advance();
        Ok(Expression::ArrayLiteral(items))
    }

    /// `{name: "Ada", age: 36}` after the consumed '{'. Keys are identifiers
    /// or string literals. One lexing wrinkle: a compact single-letter key
    /// with a number value (`{b:1}`) arrives as a cell-reference TOKEN — in
    /// key position it decomposes back into key + number value.
    fn map_literal(&mut self, position: usize) -> Result<Expression, EngineError> {
        let mut entries: Vec<MapLiteralEntry> = Vec::new();

        fn append(
            entries: &mut Vec<MapLiteralEntry>,
            key: String,
            value: Expression,
            position: usize,
        ) -> Result<(), EngineError> {
            if entries.iter().any(|e| e.key == key) {
                return Err(parse_error(format!("duplicate key '{key}'"), position));
            }
            entries.push(MapLiteralEntry { key, value });
            Ok(())
        }

        if self.current().kind == TokenKind::RightBrace {
            self.advance();
            return Ok(Expression::MapLiteral(entries));
        }
        loop {
            match &self.current().kind {
                TokenKind::CellReference { column, row, .. } => {
                    // {b:1} — the lexer saw a cell reference; here it's a key
                    // and its numeric value.
                    let (key, row) = (column.clone(), *row);
                    self.advance();
                    append(
                        &mut entries,
                        key,
                        Expression::Number(BigDecimal::from_int(row)),
                        position,
                    )?;
                }

                TokenKind::Identifier(key) | TokenKind::String(key) => {
                    let key = key.clone();
                    self.advance();
                    if self.current().kind != TokenKind::Colon {
                        return Err(parse_error(
                            format!("expected ':' after key '{key}'"),
                            self.current().position(),
                        ));
                    }
                    self.advance();
                    let value = self.comparison()?;
                    append(&mut entries, key, value, position)?;
                }

                _ => {
                    return Err(parse_error(
                        "expected a key — e.g. {name: \"Ada\", age: 36}",
                        self.current().position(),
                    ));
                }
            }

            match self.current().kind {
                TokenKind::Comma => {
                    self.advance();
                }
                TokenKind::RightBrace => break,
                _ => {
                    return Err(parse_error(
                        "expected '}' or ','",
                        self.current().position(),
                    ));
                }
            }
        }
        self.advance(); // '}'
        Ok(Expression::MapLiteral(entries))
    }

    /// Indexed reduction in math notation: `∑_i=1^10(i^2)` / `∏_i=1^5(i)`
    /// (typeable as `sigma_i=…` / `product_i=…`). Special forms — the
    /// parenthesized term is NOT evaluated eagerly; it re-evaluates per index
    /// value.
    ///
    /// Bounds are signed primaries (number, variable, cell ref, or
    /// parenthesized expression): that's what keeps the `^` separator
    /// unambiguous with exponentiation. Compound bounds need parentheses —
    /// the plaintext equivalent of LaTeX braces.
    fn math_reduction(
        &mut self,
        operation: ReductionOperation,
        index_name: String,
        position: usize,
    ) -> Result<Expression, EngineError> {
        let symbol = operation.symbol();
        if index_name.is_empty() || !index_name.chars().all(char::is_alphabetic) {
            return Err(parse_error(
                format!(
                    "the {symbol} index must be a plain variable name (e.g. {symbol}_i=1^10(i))"
                ),
                position,
            ));
        }
        if is_reserved(&index_name) {
            return Err(parse_error(
                format!("cannot use '{index_name}' as the {symbol} index"),
                position,
            ));
        }

        if self.current().kind != TokenKind::Assign {
            return Err(parse_error(
                format!("expected '=' after the {symbol} index — e.g. {symbol}_i=1^10(i)"),
                self.current().position(),
            ));
        }
        self.advance();
        let lower = self.signed_primary()?;

        if self.current().kind != TokenKind::Caret {
            return Err(parse_error(
                format!(
                    "expected '^' before the {symbol} upper bound — parenthesize compound bounds, e.g. {symbol}_i=(n-1)^10(i)"
                ),
                self.current().position(),
            ));
        }
        self.advance();
        let upper = self.signed_primary()?;

        if self.current().kind != TokenKind::LeftParen {
            return Err(parse_error(
                format!("the {symbol} term must be parenthesized — e.g. {symbol}_i=1^10(i)"),
                self.current().position(),
            ));
        }
        self.advance();
        let body = self.comparison()?;
        if self.current().kind != TokenKind::RightParen {
            return Err(parse_error("expected ')'", self.current().position()));
        }
        self.advance();

        Ok(Expression::Reduction {
            operation,
            index: index_name,
            lower: Box::new(lower),
            upper: Box::new(upper),
            body: Box::new(body),
        })
    }

    /// A ∑ bound: optional minus + bound primary. Deliberately not `unary()`
    /// (which falls into `power()` and would consume the `^` bound separator)
    /// and not `primary()` (which would treat `n(` as a call, swallowing the
    /// term's parentheses in `∑_i=1^n(i)`).
    fn signed_primary(&mut self) -> Result<Expression, EngineError> {
        if self.current().kind == TokenKind::Minus {
            self.advance();
            return Ok(Expression::UnaryMinus(Box::new(self.bound_primary()?)));
        }
        self.bound_primary()
    }

    fn bound_primary(&mut self) -> Result<Expression, EngineError> {
        let token = self.advance();
        let token_position = token.position();
        match token.kind {
            TokenKind::Number(value) => Ok(Expression::Number(value)),
            TokenKind::CellReference { column, row, .. } => Ok(Expression::CellReference {
                sheet: None,
                column,
                row,
            }),
            // Never a call — a following '(' is the term.
            TokenKind::Identifier(name) => Ok(Expression::Variable(name)),
            TokenKind::LeftParen => {
                let inner = self.comparison()?;
                if self.current().kind != TokenKind::RightParen {
                    return Err(parse_error("expected ')'", self.current().position()));
                }
                self.advance();
                Ok(inner)
            }
            _ => Err(parse_error(
                "expected a ∑ bound (number, variable, or parenthesized expression)",
                token_position,
            )),
        }
    }

    /// `Bits::BitFormat`, `A::B::c` — a namespace-qualified reference
    /// (nesting chains `::`); with `(` it's a qualified call (the constructor
    /// of a namespaced type). The whole qualified name flows as one string
    /// ("A::B::c") that the evaluator resolves.
    fn qualified_name(
        &mut self,
        namespace: String,
        _position: usize,
    ) -> Result<Expression, EngineError> {
        let mut qualified = namespace.clone();
        loop {
            self.advance(); // '::'
            let TokenKind::Identifier(member) = &self.current().kind else {
                return Err(parse_error(
                    format!("expected a name after '::' — e.g. {namespace}::Point"),
                    self.current().position(),
                ));
            };
            let member = member.clone();
            self.advance();
            qualified.push_str("::");
            qualified.push_str(&member);
            if self.current().kind != TokenKind::ColonColon {
                break;
            }
        }
        if self.current().kind != TokenKind::LeftParen {
            return Ok(Expression::Variable(qualified));
        }
        self.advance();
        Ok(Expression::Call {
            name: qualified,
            arguments: self.argument_list()?,
        })
    }

    /// After a sheet name: `!` then a cell reference, range, or 'named cell'.
    fn qualified_reference(
        &mut self,
        sheet: String,
        _position: usize,
    ) -> Result<Expression, EngineError> {
        if self.current().kind != TokenKind::Bang {
            return Err(parse_error(
                format!("expected '!' after sheet name '{sheet}' — e.g. '{sheet}'!A:1"),
                self.current().position(),
            ));
        }
        self.advance();
        match &self.current().kind {
            TokenKind::CellReference { column, row, .. } => {
                let (column, row) = (column.clone(), *row);
                self.advance();
                self.cell_reference_or_range(Some(sheet), column, row)
            }
            TokenKind::QuotedName(name) => {
                // Budget!'Projected Rate'
                let name = name.clone();
                self.advance();
                Ok(Expression::NameReference {
                    sheet: Some(sheet),
                    name,
                })
            }
            _ => Err(parse_error(
                format!("expected a cell or 'named cell' after '{sheet}!' — e.g. {sheet}!A:1"),
                self.current().position(),
            )),
        }
    }

    /// A (possibly qualified) cell, optionally extended to a range by `..`.
    fn cell_reference_or_range(
        &mut self,
        sheet: Option<String>,
        column: String,
        row: i64,
    ) -> Result<Expression, EngineError> {
        if self.current().kind != TokenKind::DotDot {
            return Ok(Expression::CellReference { sheet, column, row });
        }
        self.advance();
        let TokenKind::CellReference {
            column: to_column,
            row: to_row,
            ..
        } = &self.current().kind
        else {
            return Err(parse_error(
                "expected a cell after '..' (e.g. A:1..A:9)",
                self.current().position(),
            ));
        };
        let (to_column, to_row) = (to_column.clone(), *to_row);
        self.advance();
        Ok(Expression::CellRange {
            sheet,
            from_column: column,
            from_row: row,
            to_column,
            to_row,
        })
    }

    /// Arguments after a consumed '(' — empty list allowed (`pi()` style not
    /// required, but `rand()` future-proofing is free).
    fn argument_list(&mut self) -> Result<Vec<Expression>, EngineError> {
        if self.current().kind == TokenKind::RightParen {
            self.advance();
            return Ok(Vec::new());
        }
        // Named arguments — Person(name: "Ada", age: 36) — desugar to ONE map
        // literal, which makes them exactly the from-map constructor form.
        if self.is_named_argument_start() {
            return Ok(vec![self.named_arguments()?]);
        }
        let mut arguments = vec![self.comparison()?];
        while self.current().kind == TokenKind::Comma {
            self.advance();
            arguments.push(self.comparison()?);
        }
        if self.current().kind != TokenKind::RightParen {
            return Err(parse_error(
                "expected ')' or ','",
                self.current().position(),
            ));
        }
        self.advance();
        Ok(arguments)
    }

    /// Does this argument list open with `name: value`? Two shapes commit:
    /// an identifier directly followed by ':', or a compact `age:36` that the
    /// lexer fused into a cell-reference token with a MULTI-letter column —
    /// real columns are single letters, so that can't be a cell. A compact
    /// single-letter `f(a:1)` stays a cell reference; write `f(a: 1)`.
    fn is_named_argument_start(&self) -> bool {
        if matches!(self.current().kind, TokenKind::Identifier(_))
            && self.kind_at(self.index + 1) == Some(&TokenKind::Colon)
        {
            return true;
        }
        if let TokenKind::CellReference { column, .. } = &self.current().kind {
            if column.chars().count() > 1 {
                return true;
            }
        }
        false
    }

    /// `name: "Ada", age: 36)` after the consumed '(' — consumes the ')'.
    /// Same lexing wrinkle as map literals: a compact `key:number` arrives as
    /// one cell-reference token and decomposes back into key + value.
    fn named_arguments(&mut self) -> Result<Expression, EngineError> {
        let position = self.current().position();
        let mut entries: Vec<MapLiteralEntry> = Vec::new();

        fn append(
            entries: &mut Vec<MapLiteralEntry>,
            key: String,
            value: Expression,
            position: usize,
        ) -> Result<(), EngineError> {
            if entries.iter().any(|e| e.key == key) {
                return Err(parse_error(format!("duplicate field '{key}'"), position));
            }
            entries.push(MapLiteralEntry { key, value });
            Ok(())
        }

        loop {
            match &self.current().kind {
                TokenKind::CellReference { column, row, .. } => {
                    let (key, row) = (column.clone(), *row);
                    self.advance();
                    append(
                        &mut entries,
                        key,
                        Expression::Number(BigDecimal::from_int(row)),
                        position,
                    )?;
                }

                TokenKind::Identifier(key) => {
                    let key = key.clone();
                    self.advance();
                    if self.current().kind != TokenKind::Colon {
                        return Err(parse_error(
                            format!("expected ':' after '{key}' — named arguments are name: value"),
                            self.current().position(),
                        ));
                    }
                    self.advance();
                    let value = self.comparison()?;
                    append(&mut entries, key, value, position)?;
                }

                _ => {
                    return Err(parse_error(
                        "expected another name: value argument",
                        self.current().position(),
                    ));
                }
            }

            match self.current().kind {
                TokenKind::Comma => {
                    self.advance();
                }
                TokenKind::RightParen => break,
                _ => {
                    return Err(parse_error(
                        "expected ')' or ','",
                        self.current().position(),
                    ));
                }
            }
        }
        self.advance(); // ')'
        Ok(Expression::MapLiteral(entries))
    }
}

/// The operator symbols that can name an overload definition — the six
/// arithmetic binary operators. Comparisons/equality are not overloadable.
/// (`%` is postfix percent, not an overloadable operator.)
fn operator_definition_name(kind: &TokenKind) -> Option<&'static str> {
    match kind {
        TokenKind::Plus => Some("+"),
        TokenKind::Minus => Some("-"),
        TokenKind::Star => Some("*"),
        TokenKind::Slash => Some("/"),
        TokenKind::Caret => Some("^"),
        _ => None,
    }
}

fn comparison_operator(kind: &TokenKind) -> Option<ComparisonOperator> {
    match kind {
        TokenKind::LessThan => Some(ComparisonOperator::Less),
        TokenKind::GreaterThan => Some(ComparisonOperator::Greater),
        TokenKind::LessOrEqual => Some(ComparisonOperator::LessOrEqual),
        TokenKind::GreaterOrEqual => Some(ComparisonOperator::GreaterOrEqual),
        TokenKind::EqualEqual => Some(ComparisonOperator::Equal),
        TokenKind::NotEqual => Some(ComparisonOperator::NotEqual),
        _ => None,
    }
}

#[cfg(test)]
mod tests;
