type TomlDocument = toml_edit::DocumentMut;

pub trait TomlItemExt {
    fn make_table_inline(&mut self);
    fn make_table_regular(&mut self);
}

/// Convenience impl based on <https://github.com/toml-rs/toml/blob/1189a129ba8c672708f555c855561dc65edffdda/crates/toml_edit/examples/visit.rs#L121>
impl TomlItemExt for toml_edit::Item {
    fn make_table_inline(&mut self) {
        if let toml_edit::Item::Table(table) = self {
            // Turn the table into an inline table.
            let table = std::mem::replace(table, toml_edit::Table::new());
            let inline_table = table.into_inline_table();
            *self = toml_edit::Item::Value(toml_edit::Value::InlineTable(inline_table));
        }
    }

    fn make_table_regular(&mut self) {
        if let toml_edit::Item::Value(toml_edit::Value::InlineTable(inline_table)) = self {
            let inline_table = std::mem::replace(inline_table, toml_edit::InlineTable::new());
            let table = inline_table.into_table();
            *self = toml_edit::Item::Table(table);
        }
    }
}
