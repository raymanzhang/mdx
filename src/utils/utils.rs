//! Utility functions for MDX dictionary processing.
//!
//! This module provides various helper functions for:
//! - XML to JSON conversion
//! - HTML text processing and escaping
//! - String comparison and sorting
//! - LinkedList manipulation
//! - XML declaration handling
//!
//! # Examples
//!
//! ```
//! use mdx::utils::{remove_xml_declaration, html_escape_mdx_text};
//!
//! // Remove XML declaration from a string
//! let xml = "<?xml version=\"1.0\"?><root>data</root>";
//! let cleaned = remove_xml_declaration(xml);
//! assert_eq!(cleaned, "<root>data</root>");
//!
//! // Escape HTML text
//! let mut output = String::new();
//! html_escape_mdx_text("Hello <world>", &mut output);
//! assert!(output.contains("Hello"));
//! ```

use std::cell::RefCell;
use std::cmp::Ordering;
use std::collections::LinkedList;

use lol_html::{text, HtmlRewriter, Settings};
use quick_xml::events::Event;
use serde_json::{Map, Value};

use crate::storage::meta_unit::MetaUnit;
use crate::storage::reader_helper;
use super::sort_key::get_sort_key;
use crate::{Result, ZdbError};

/// Moves an element in a `LinkedList` from position `pos` to position `new_pos`.
/// If `pos` is out of bounds, the list remains unchanged.
/// If `new_pos` is out of bounds, the element is moved to the end of the list.
/// If `pos` equals `new_pos` or the list is empty, no changes are made.
///
/// # Examples
///
/// ```
/// use std::collections::LinkedList;
/// use mdx::utils::move_element;
/// let cases = [
///     (vec![1, 2, 3, 4], 1, 3, vec![1, 3, 4, 2]),
///     (vec![1, 2, 3, 4], 0, 2, vec![2, 3, 1, 4]),
///     (vec![1, 2, 3, 4], 2, 0, vec![1, 3, 2, 4]),
///     (vec![1, 2, 3, 4], 3, 1, vec![1, 2, 4, 3]),
///     (vec![1, 2, 3, 4], 2, 2, vec![1, 2, 3, 4]),
///     (vec![1, 2, 3, 4], 10, 2, vec![1, 2, 3, 4]),
///     (vec![1, 2, 3, 4], 1, 10, vec![1, 3, 4, 2]),
/// ];
/// for (input, pos, new_pos, expected) in cases {
///     let mut list: LinkedList<_> = input.into_iter().collect();
///     move_element(&mut list, pos, new_pos);
///     let result: Vec<_> = list.into_iter().collect();
///     assert_eq!(result, expected);
/// }
/// ```
pub fn move_element<T>(list: &mut LinkedList<T>, pos: usize, new_pos: usize) {
    if pos == new_pos || list.is_empty() {
        return;
    }
    let len = list.len();
    if pos >= len {
        return;
    }

    // 1. Remove the element at position pos
    let mut removed = None;
    for i in 0..len {
        let v = list.pop_front().unwrap();
        if i == pos {
            removed = Some(v);
        } else {
            list.push_back(v);
        }
    }
    let mut elem = Some(removed.unwrap());

    // 2. Calculate insertion position (move to after new_pos element)
    let mut insert_pos = new_pos + 1;
    if pos < insert_pos {
        insert_pos -= 1;
    }
    if insert_pos > list.len() {
        insert_pos = list.len();
    }

    // 3. Insert at insert_pos
    let new_len = list.len();
    for i in 0..=new_len {
        if i == insert_pos {
            list.push_back(elem.take().unwrap());
        }
        if i < new_len {
            let v = list.pop_front().unwrap();
            list.push_back(v);
        }
    }
}

#[inline]
fn string_from_slice(utf8: &[u8]) -> String {
    String::from_utf8_lossy(utf8).into_owned()
}

pub fn simple_xml_to_json(xml: &str) -> Result<serde_json::Value> {
    let mut reader = quick_xml::Reader::from_str(xml);
    let mut json_map = Map::new();

    loop {
        match reader.read_event() {
            Ok(Event::Empty(ref e)) => {
                let name = string_from_slice(e.name().as_ref());
                let mut attrs = Map::new();
                for attr in e.attributes() {
                    match attr {
                        Ok(attr) => {
                            let key = string_from_slice(attr.key.as_ref());
                            let value = string_from_slice(&attr.value);
                            attrs.insert(key, Value::String(value));
                        }
                        _ => { continue; }
                    }
                }
                json_map.insert(name, Value::Object(attrs));
                break; // Exit after processing single empty element
            }
            Ok(Event::Eof) => break,
            Err(e) => return Err(e.into()),
            _ => {}
        }
    }
    Ok(Value::Object(json_map))
}

pub fn remove_xml_declaration(xml: &mut String) {
    if xml.starts_with("<?xml") {
        if let Some(end) = xml.find("?>") { //remove XML declaration
            *xml = xml[end + 2..].trim_start().to_string();
        }
    }
}

// Trait for comparison operations
pub trait KeyComparable {
    fn compare_with(&self, other:&str, other_sort_key:&[u8], start_with: bool, meta_info: &MetaUnit) -> Result<Ordering>;
}
pub trait RandomAccessable<T:KeyComparable>{
    fn get_item(&self, index:usize)->Result<&T>;
    fn len(&self)->usize;
}
//compare the first sort key with the second sort key
//if prefix_match is true and second sort key start with first sort key, return equal
pub fn sort_key_compare( first: &[u8], second: &[u8], start_with: bool) -> Result<Ordering> {
    let first = if start_with && first.len() > second.len() {
        &first[..second.len()]
    } else {
        first
    };
    Ok(first.cmp(second))
}

pub fn locale_compare(first: &str, second: &str, start_with: bool, meta_info: &MetaUnit) -> Result<Ordering> {
    let first = if start_with && first.len() > second.len() {
        let char_count = second.chars().count();
        let end = first.char_indices()
        .nth(char_count) // Get byte position of nth character
        .map(|(i, _)| i) // Extract byte index
        .unwrap_or(first.len()); // If less than n characters, return full string length
        &first[..end]
    } else {
        first
    };
    let result = meta_info.collator.as_ref().strcoll_utf8(first, second)?;
    Ok(result)
}

pub fn key_compare(first: &str, first_sort_key: &[u8], second: &str, second_sort_key: &[u8], start_with: bool, meta_info: &MetaUnit) -> Result<Ordering> {
    if meta_info.is_v3(){
        //Because ICU4x doesn't provide sort key support, and the icu4c's get_sort_key is slow according to their documentation,
        //we use locale compare directly here.
        locale_compare(first, second, start_with, meta_info)
    }else{
        sort_key_compare(first_sort_key, second_sort_key, start_with)
    }
}

pub fn binary_search_first<T: KeyComparable+Clone, C:RandomAccessable<T>>(
    container: &C,
    key: &str,
    meta_info: &MetaUnit,
    prefix_match: bool,
    partial_match: bool,
) -> Result<Option<T>> {
    let mut search_key = key.to_string();
    let search_key_bytes = reader_helper::encode_string_to_bytes(&search_key, meta_info.encoding_obj)?;
    let mut search_sort_key = get_sort_key(&search_key_bytes, meta_info)?;
    let mut result = None;

    while result.is_none() && !search_key.is_empty() {
        let mut left = 0;
        let mut right = container.len();
        let mut found_index = None;

        while left < right {
            let mid = (left + right) / 2;
            let mid_item = container.get_item(mid)?;

            match mid_item.compare_with(&search_key, &search_sort_key, prefix_match, meta_info)? {
                Ordering::Less => left = mid + 1,
                Ordering::Greater => right = mid,
                Ordering::Equal => {
                    found_index = Some(mid);
                    right = mid; // Continue searching left for the leftmost match
                }
            }
        }

        if let Some(index) = found_index {
            // Found a match, now search leftward for the leftmost match
            let mut leftmost_index = index;
            while leftmost_index > 0 {
                let prev_item = container.get_item(leftmost_index - 1)?;
                match prev_item.compare_with(&search_key, &search_sort_key, prefix_match, meta_info) {
                    Ok(Ordering::Equal) => leftmost_index -= 1,
                    _ => break,
                }
            }
            result = Some(container.get_item(leftmost_index)?.clone());
        } else if partial_match {
            // If no match found and partial_match is enabled, try with a shorter key
            if search_key.len() > 0 {
                search_key.pop();
                search_sort_key = get_sort_key(&search_key.as_bytes(), meta_info)?;
            } else {
                break;
            }
        } else {
            break;
        }
    }
    Ok(result)
}

/// Escapes HTML special characters in MDX text content and appends to the provided string.
/// 
/// This function converts special characters to their HTML entity equivalents:
/// - `\n` → `<br>`
/// - `&` → `&amp;`
/// - `<` → `&lt;`
/// - `>` → `&gt;`
/// - `"` → `&quot;`
/// - `\n` (escaped newline) → `<br>`
/// 
/// # Arguments
/// 
/// * `mdx_text` - The input MDX text to escape
/// * `escaped_text` - The mutable string to append the escaped content to
/// 
/// # Examples
/// 
/// ```
/// use mdx::utils::html_escape_mdx_text;
/// 
/// let mut result = String::from("Prefix: ");
/// html_escape_mdx_text("Hello & <world>\nNext line", &mut result);
/// assert_eq!(result, "Prefix: Hello &amp; &lt;world&gt;<br>Next line");
/// 
/// let mut result = String::new();
/// html_escape_mdx_text("Line 1\\nLine 2", &mut result);
/// assert_eq!(result, "Line 1<br>Line 2");
/// ```
pub fn html_escape_mdx_text(mdx_text: &str, escaped_text: &mut String) {
    let chars: Vec<char> = mdx_text.chars().collect();
    let mut i = 0;
    
    while i < chars.len() {
        let ch = chars[i];
        
        match ch {
            '\n' => {
                escaped_text.push_str("<br>");
            }
            '&' => {
                escaped_text.push_str("&amp;");
            }
            '<' => {
                escaped_text.push_str("&lt;");
            }
            '>' => {
                escaped_text.push_str("&gt;");
            }
            '"' => {
                escaped_text.push_str("&quot;");
            }
            '\\' => {
                // Check for "\n"
                if i + 1 < chars.len() && chars[i + 1] == 'n' {
                    escaped_text.push_str("<br>");
                    i += 1; // Skip the 'n' character
                } else {
                    escaped_text.push(ch);
                }
            }
            _ => {
                escaped_text.push(ch);
            }
        }
        
        i += 1;
    }
}

/// Extract text content from HTML using lol_html for efficient streaming parsing
pub fn extract_text_from_html(html: &str) -> Result<String> {
    let text_content = RefCell::new(String::new());
    
    // Create HTML rewriter settings with text handler
    let settings = Settings {
        element_content_handlers: vec![
            // Handle text content
            text!("*", {
                let content = &text_content;
                move |text| {
                    content.borrow_mut().push_str(text.as_str());
                    content.borrow_mut().push(' ');
                    Ok(())
                }
            }),
        ],
        ..Settings::default()
    };
    
    // Create HTML rewriter and process the HTML
    let mut extracter = HtmlRewriter::new(settings, |_c: &[u8]| {
        // This callback is called for any content that wasn't handled by handlers
        // We don't need to do anything here since we're only interested in text
    });
    
    // Process the HTML content
    extracter.write(html.as_bytes())
        .map_err(|e| ZdbError::general_error(format!("HTML rewriting error: {}", e)))?;
    
    extracter.end()
        .map_err(|e| ZdbError::general_error(format!("HTML rewriting end error: {}", e)))?;

    let final_text = text_content.into_inner();
    
    // Clean up whitespace
    let cleaned = final_text
        .split_whitespace()
        .collect::<Vec<&str>>()
        .join(" ");
    
    Ok(cleaned)
}

/// Convert HTML to plain text, fallback to original string if conversion fails
pub fn html_to_text(html: &str) -> String {
    extract_text_from_html(html).unwrap_or_else(|_| html.to_string())
}