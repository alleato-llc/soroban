//! Statement-level productions: assignments, `import`, `namespace`/`data`
//! declarations, field-type parsing, lambdas, and user-function definitions.

use super::{is_reserved, parse_error, Parser};
use crate::ast::{Expression, Parameter, TypeAnnotation};
use crate::eval::data_type::{DataField, DataFieldType};
use crate::lexer::TokenKind;
use crate::EngineError;

impl Parser {
    /// `x = expr`, `f(a, b) = expr`, or a plain expression.
    pub(super) fn statement(&mut self) -> Result<Expression, EngineError> {
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

    /// Speculatively parses `x -> body` or `(a, b) -> body` (also
    /// `() -> body`). Returns `None` — position rewound — when the shape
    /// doesn't end in `->`, so `(a, b)` stays a parenthesized expression and
    /// `x` stays a variable.
    pub(super) fn lambda_expression(&mut self) -> Result<Option<Expression>, EngineError> {
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
