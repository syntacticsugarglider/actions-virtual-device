use std::fmt::Display;

pub fn format_list<I: IntoIterator<Item = T>, T: Display>(data: I) -> String {
    let mut iter = data.into_iter().peekable();
    let mut data = String::new();
    if let Some(item) = iter.next() {
        data.push_str(&format!("{}", item))
    }
    if let Some(item) = iter.next() {
        if let None = iter.peek() {
            data.push_str(&format!(" and {}", item))
        } else {
            data.push_str(&format!(", {}", item))
        }
    }
    while let Some(item) = iter.next() {
        if let None = iter.peek() {
            data.push_str(&format!(", and {}", item))
        } else {
            data.push_str(&format!(", {}", item))
        }
    }
    data
}
