use sfv::{BareItem, Item, ListEntry, Parser, SerializeValue};

fn main() {
    let input = "\"v1\", \"v2\", 123";
    let list = Parser::parse_list(input.as_bytes()).unwrap();
    for member in list {
        match member {
            ListEntry::Item(item) => match item.bare_item {
                BareItem::String(s) => println!("String: {}", s),
                BareItem::Integer(i) => println!("Integer: {}", i),
                _ => println!("Other item"),
            },
            ListEntry::InnerList(_) => println!("Inner list"),
        }
    }
}
