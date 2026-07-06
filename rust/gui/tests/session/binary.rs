//! Binary bit-editor steps: opening/editing the register, width picker, bit
//! formats and their fields, and the visual format builder (Build / Save).

use crate::SessionWorld;
use cucumber::{then, when};
use soroban_engine::FormatBuilderFieldKind;
use soroban_gui::session::{BinaryFieldKind, BinaryStatus};

// MARK: Binary bit editor

#[when("I open the bit editor")]
fn i_open_bit_editor(world: &mut SessionWorld) {
    world.session.refresh_binary();
}

#[then("the bit editor is editable")]
fn bit_editor_is_editable(world: &mut SessionWorld) {
    assert!(
        matches!(world.session.binary_status(), BinaryStatus::Editable { .. }),
        "the bit editor is not editable"
    );
}

#[then("the bit editor is not editable")]
fn bit_editor_not_editable(world: &mut SessionWorld) {
    assert!(
        matches!(world.session.binary_status(), BinaryStatus::Unavailable(_)),
        "the bit editor is unexpectedly editable"
    );
}

#[when(regex = r#"^I flip bit ([0-9]+)$"#)]
fn i_flip_bit(world: &mut SessionWorld, index: String) {
    let index: usize = index.parse().expect("bit index must be a number");
    world.session.flip_binary_bit(index);
}

#[then(regex = r#"^the bit editor value is "(.*)"$"#)]
fn bit_editor_value_is(world: &mut SessionWorld, expected: String) {
    match world.session.binary_status() {
        BinaryStatus::Editable { value, .. } => {
            assert_eq!(
                value, expected,
                "bit editor value is '{value}', expected '{expected}'"
            )
        }
        BinaryStatus::Unavailable(reason) => panic!("bit editor unavailable: {reason}"),
    }
}

#[when("I use the bit editor value")]
fn i_use_bit_editor(world: &mut SessionWorld) {
    world.session.use_binary();
}

/// Assert the LSB-first bit ordering the widget relies on: bit 0 is `bits[0]`.
/// (This pins the fix for "clicking the first bit flipped the last one".)
#[then(regex = r#"^bit ([0-9]+) of the editor is (set|clear)$"#)]
fn bit_of_editor_is(world: &mut SessionWorld, index: String, state: String) {
    let index: usize = index.parse().expect("bit index must be a number");
    match world.session.binary_status() {
        BinaryStatus::Editable { bits, .. } => {
            let expected = state == "set";
            assert_eq!(
                bits[index],
                expected,
                "bit {index} is {}, expected {state}",
                if bits[index] { "set" } else { "clear" }
            );
        }
        BinaryStatus::Unavailable(reason) => panic!("bit editor unavailable: {reason}"),
    }
}

#[then(regex = r#"^the bit editor reads hex "(.*)"$"#)]
fn bit_editor_reads_hex(world: &mut SessionWorld, expected: String) {
    match world.session.binary_status() {
        BinaryStatus::Editable { hex, .. } => {
            assert_eq!(
                hex, expected,
                "bit editor hex is '{hex}', expected '{expected}'"
            )
        }
        BinaryStatus::Unavailable(reason) => panic!("bit editor unavailable: {reason}"),
    }
}

#[then(regex = r#"^the bit editor width is ([0-9]+)$"#)]
fn bit_editor_width_is(world: &mut SessionWorld, expected: String) {
    let expected: u32 = expected.parse().expect("width must be a number");
    match world.session.binary_status() {
        BinaryStatus::Editable { width, .. } => {
            assert_eq!(
                width, expected,
                "bit editor width is {width}, expected {expected}"
            )
        }
        BinaryStatus::Unavailable(reason) => panic!("bit editor unavailable: {reason}"),
    }
}

#[when(regex = r#"^I set the bit editor width to ([0-9]+)$"#)]
fn i_set_bit_editor_width(world: &mut SessionWorld, width: String) {
    let width: u32 = width.parse().expect("width must be a number");
    world.session.set_binary_width(width);
}

#[then(regex = r#"^the width ([0-9]+) is (offered|disabled)$"#)]
fn width_is_offered(world: &mut SessionWorld, bits: String, state: String) {
    let bits: u32 = bits.parse().expect("width must be a number");
    let width = world
        .session
        .binary_widths()
        .into_iter()
        .find(|w| w.bits == bits)
        .unwrap_or_else(|| panic!("width {bits} is not in the picker"));
    let expected_enabled = state == "offered";
    assert_eq!(
        width.enabled, expected_enabled,
        "width {bits} enabled = {}, expected {state}",
        width.enabled
    );
}

#[then(regex = r#"^the bit format picker offers "(.*)"$"#)]
fn picker_offers(world: &mut SessionWorld, name: String) {
    assert!(
        world.session.binary_preset_names().contains(&name),
        "the format picker does not offer '{name}'"
    );
}

#[then("the width picker is empty")]
fn width_picker_empty(world: &mut SessionWorld) {
    assert!(
        world.session.binary_widths().is_empty(),
        "expected no width choices (a locked value), found some"
    );
}

#[when(regex = r#"^I apply the "(.*)" bit format$"#)]
fn i_apply_bit_format(world: &mut SessionWorld, name: String) {
    world.session.apply_binary_format(Some(&name));
}

#[when("I clear the bit format")]
fn i_clear_bit_format(world: &mut SessionWorld) {
    world.session.apply_binary_format(None);
}

#[then(regex = r#"^the active bit format is "(.*)"$"#)]
fn active_bit_format_is(world: &mut SessionWorld, expected: String) {
    let name = world.session.binary_format_name().unwrap_or_default();
    assert_eq!(
        name, expected,
        "active format is '{name}', expected '{expected}'"
    );
}

#[then("there is no active bit format")]
fn no_active_bit_format(world: &mut SessionWorld) {
    assert!(
        world.session.binary_format_name().is_none(),
        "expected no active format, found {:?}",
        world.session.binary_format_name()
    );
}

#[then(regex = r#"^the bit format has a field "(.*)" reading "(.*)"$"#)]
fn field_reads(world: &mut SessionWorld, name: String, label: String) {
    let fields = world.session.binary_fields();
    let field = fields
        .iter()
        .find(|f| f.name == name)
        .unwrap_or_else(|| panic!("no field named '{name}' (have {fields:?})"));
    assert_eq!(
        field.label, label,
        "field '{name}' reads '{}', expected '{label}'",
        field.label
    );
}

#[then(regex = r#"^the bit format field "(.*)" sits at bit ([0-9]+) for ([0-9]+) bits$"#)]
fn field_range(world: &mut SessionWorld, name: String, low: String, width: String) {
    let low: u32 = low.parse().expect("bit index must be a number");
    let width: u32 = width.parse().expect("width must be a number");
    let fields = world.session.binary_fields();
    let field = fields
        .iter()
        .find(|f| f.name == name)
        .unwrap_or_else(|| panic!("no field named '{name}' (have {fields:?})"));
    assert_eq!(field.low_bit, low, "field '{name}' low bit");
    assert_eq!(field.width, width, "field '{name}' width");
}

/// Look up one field of the active format by name, panicking if it's absent.
fn field_named(world: &mut SessionWorld, name: &str) -> soroban_gui::session::BinaryFieldView {
    world
        .session
        .binary_fields()
        .into_iter()
        .find(|f| f.name == name)
        .unwrap_or_else(|| panic!("no field named '{name}'"))
}

#[then(regex = r#"^the bit format field "(.*)" has kind (numeric|flags|enum|reserved|unused)$"#)]
fn field_has_kind(world: &mut SessionWorld, name: String, kind: String) {
    let expected = match kind.as_str() {
        "numeric" => BinaryFieldKind::Numeric,
        "flags" => BinaryFieldKind::Flags,
        "enum" => BinaryFieldKind::Enum,
        "reserved" => BinaryFieldKind::Reserved,
        "unused" => BinaryFieldKind::Unused,
        other => panic!("unknown kind '{other}'"),
    };
    let field = field_named(world, &name);
    assert_eq!(field.kind, expected, "field '{name}' kind");
}

#[when(regex = r#"^I set bit format field "(.*)" to "(.*)"$"#)]
fn i_set_field(world: &mut SessionWorld, name: String, text: String) {
    assert!(
        world.session.set_binary_field(&name, &text),
        "setting field '{name}' to '{text}' failed"
    );
}

#[then(regex = r#"^setting bit format field "(.*)" to "(.*)" is rejected$"#)]
fn setting_field_rejected(world: &mut SessionWorld, name: String, text: String) {
    assert!(
        !world.session.set_binary_field(&name, &text),
        "expected setting field '{name}' to '{text}' to be rejected"
    );
}

#[then(regex = r#"^the bit format field "(.*)" offers "(.*)"$"#)]
fn field_offers(world: &mut SessionWorld, name: String, option: String) {
    let field = field_named(world, &name);
    assert!(
        field.options.contains(&option),
        "field '{name}' does not offer '{option}' (has {:?})",
        field.options
    );
}

#[then(regex = r#"^the bit format field "(.*)" is selected as index ([0-9]+)$"#)]
fn field_selected_index(world: &mut SessionWorld, name: String, index: String) {
    let index: usize = index.parse().expect("index must be a number");
    let field = field_named(world, &name);
    assert_eq!(field.selected, Some(index), "field '{name}' selected index");
}

#[then(regex = r#"^the bit format field "(.*)" flag "(.*)" is (set|clear)$"#)]
fn field_flag_is(world: &mut SessionWorld, name: String, flag: String, state: String) {
    let field = field_named(world, &name);
    let bit = field
        .flags
        .iter()
        .find(|b| b.name == flag)
        .unwrap_or_else(|| panic!("field '{name}' has no flag '{flag}'"));
    assert_eq!(
        bit.set,
        state == "set",
        "field '{name}' flag '{flag}' is {}",
        if bit.set { "set" } else { "clear" }
    );
}

// MARK: Format builder (Build / Save custom formats)

#[when("I begin building a new bit format")]
fn i_begin_new_format(world: &mut SessionWorld) {
    world.session.begin_format_build(false);
}

#[when("I begin editing the current bit format")]
fn i_begin_edit_format(world: &mut SessionWorld) {
    world.session.begin_format_build(true);
}

/// Claim `bits`, describe the pending field, and add it — one step so a
/// scenario reads as "add a field", not five builder pokes.
#[when(
    regex = r#"^I add an? (numeric|flags|enum|reserved|unused) field "(.*)" of ([0-9]+) bits(?: labelled "(.*)")?$"#
)]
fn i_add_field(world: &mut SessionWorld, kind: String, name: String, bits: String, labels: String) {
    let bits: u32 = bits.parse().expect("bits must be a number");
    let kind = match kind.as_str() {
        "numeric" => FormatBuilderFieldKind::Numeric,
        "flags" => FormatBuilderFieldKind::Flags,
        "enum" => FormatBuilderFieldKind::Enumeration,
        "reserved" => FormatBuilderFieldKind::Reserved,
        "unused" => FormatBuilderFieldKind::Unused,
        other => panic!("unknown builder kind '{other}'"),
    };
    let builder = world
        .session
        .format_builder_mut()
        .expect("no builder is open");
    builder.claim(bits);
    builder.draft_name = name;
    builder.draft_kind = kind;
    builder.draft_labels = labels;
    builder.add_field();
}

#[then(regex = r#"^the builder has ([0-9]+) fields$"#)]
fn builder_has_fields(world: &mut SessionWorld, count: String) {
    let count: usize = count.parse().expect("count must be a number");
    let builder = world.session.format_builder().expect("no builder is open");
    assert_eq!(builder.fields().len(), count, "builder field count");
}

#[when("I apply the built format")]
fn i_apply_built(world: &mut SessionWorld) {
    world.session.apply_built_format();
}

#[when(regex = r#"^I save the format as "(.*)"$"#)]
fn i_save_format(world: &mut SessionWorld, name: String) {
    assert!(
        world.session.save_format(&name),
        "saving format '{name}' failed"
    );
}

#[then(regex = r#"^saving the format as "(.*)" is rejected$"#)]
fn saving_format_rejected(world: &mut SessionWorld, name: String) {
    assert!(
        !world.session.save_format(&name),
        "expected saving format '{name}' to be rejected"
    );
}

#[when(regex = r#"^I delete the saved format "(.*)"$"#)]
fn i_delete_saved(world: &mut SessionWorld, name: String) {
    world.session.delete_saved_format(&name);
}

#[then(regex = r#"^the saved formats include "(.*)"$"#)]
fn saved_formats_include(world: &mut SessionWorld, name: String) {
    assert!(
        world.session.saved_format_names().contains(&name),
        "saved formats do not include '{name}' (have {:?})",
        world.session.saved_format_names()
    );
}

#[then(regex = r#"^the saved formats exclude "(.*)"$"#)]
fn saved_formats_exclude(world: &mut SessionWorld, name: String) {
    assert!(
        !world.session.saved_format_names().contains(&name),
        "saved formats unexpectedly include '{name}'"
    );
}
