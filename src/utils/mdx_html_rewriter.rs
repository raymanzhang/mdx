//! HTML rewriter for MDX dictionary content.
//!
//! This module provides high-performance streaming HTML link rewriting using lol_html 2.6.0.
//! It converts various dictionary-specific URL schemes to the MDX protocol format.
//!
//! # Supported Link Type Conversions
//!
//! - `entry://` → `mdx://mdict.cn/service/entry?profile_id=&key=`
//! - `entryx://` → `mdx://mdict.cn/service/entryx?profile_id=&entry_no=`
//! - `sound://` → `mdx://mdict.cn/service/sound?profile_id=&key=`
//! - `source://` → `mdx://mdict.cn/service/source?profile_id=&entry_no=`
//! - `file://` or no protocol → `mdx://mdict.cn/service/mdd?profile_id=&key=`
//!
//! # Examples
//! 
//! ```rust
//! use mdx::mdx_html_rewriter::MdxHtmlRewriter;
//! 
//! let html_input = r#"<img src="entry://test.png"><a href="sound://music.mp3">Audio</a>"#;
//! 
//! // One-shot string processing - suitable for all HTML content
//! let result = MdxHtmlRewriter::rewrite_html(html_input, 123).unwrap();
//! // Result: <img src="mdx://mdict.cn/service/entry?profile_id=123&key=test.png"><a href="mdx://mdict.cn/service/sound?profile_id=123&key=music.mp3">Audio</a>
//! 
//! // Custom base URL
//! let result = MdxHtmlRewriter::rewrite_html_with_base_url(html_input, 123, "custom://my-domain.com").unwrap();
//! // Result: <img src="custom://my-domain.com/entry?profile_id=123&key=test.png">...
//! ```

use lol_html::{element, rewrite_str, Settings};
use percent_encoding;
use url::Url;

use crate::Result;

const DEFAULT_BASE_URL: &'static str = "mdx://mdict.cn/service/";

/// HTML rewriter for MDX dictionary content.
pub struct MdxHtmlRewriter;

/// Macro to create element handlers, avoiding code duplication.
macro_rules! create_handlers {
    ($profile_id:expr, $base_url:expr) => {{
        // Link attributes that need to be processed
        const LINK_ATTRIBUTES: &[&str] = &[
            "href", "src", "background", "background-image", "poster", "data",
            "action", "cite", "codebase", "usemap", "longdesc", "archive", "classid"
        ];
        
        // Generate selector for all attributes, e.g., "*[href], *[src], *[background], ..."
        let selector = LINK_ATTRIBUTES.iter()
            .map(|attr| format!("*[{}]", attr))
            .collect::<Vec<_>>()
            .join(", ");
                
        vec![
            // Unified processing for all link attributes
            element!(&selector, move |el| {
                for &attr in LINK_ATTRIBUTES {
                    if let Some(value) = el.get_attribute(attr) {
                        let new_value = MdxHtmlRewriter::rewrite_url(&value, $profile_id, &$base_url);
                        el.set_attribute(attr, &new_value)?;
                    }
                }
                Ok(())
            }),
            // Separate handling for CSS style attribute
            element!("*[style]", move |el| {
                if let Some(style) = el.get_attribute("style") {
                    let new_style = MdxHtmlRewriter::rewrite_css_urls(&style, $profile_id, &$base_url);
                    el.set_attribute("style", &new_style)?;
                }
                Ok(())
            }),
        ]
    }};
}

impl MdxHtmlRewriter {
    
    /// 重写HTML字符串中的链接，使用默认基础URL
    /// 
    /// 将HTML内容中的各种链接协议转换为mdx协议格式
    pub fn rewrite_html(html: &str, profile_id: i32) -> Result<String> {
        Self::rewrite_html_with_base_url(html, profile_id, DEFAULT_BASE_URL)
    }

    /// 重写HTML字符串中的链接，使用自定义基础URL
    /// 
    /// 将HTML内容中的各种链接协议转换为mdx协议格式
    pub fn rewrite_html_with_base_url(html: &str, profile_id: i32, base_url: &str) -> Result<String> {
        let rewritten = rewrite_str(
            html, 
            Settings {
                element_content_handlers: create_handlers!(profile_id, base_url),
                ..Settings::default()
            }
        ).map_err(|e| {
            crate::ZdbError::invalid_data_format(&format!("Failed to rewrite HTML: {}", e))
        })?;

        Ok(rewritten)
    }

    /// 重写单个URL，使用URL库进行标准化解析和编码
    pub fn rewrite_url(url: &str, profile_id: i32, base_url: &str) -> String {
        let url = url.trim();
        
        // 空字符串或空白字符
        if url.is_empty() {
            return url.to_string();
        }

        // 锚点链接
        if url.starts_with('#') {
            return url.to_string();
        }

        // 特殊处理：entry://#fragment 或 entry:///#fragment 形式
        // 直接去掉 entry:// 前缀，只保留 #fragment 部分
        if url.starts_with("entry://#") {
            return url[8..].to_string(); // 去掉 "entry://" 保留 "#fragment"
        }
        if url.starts_with("entry:///#") {
            return url[9..].to_string(); // 去掉 "entry:///" 保留 "#fragment"
        }

        // 需要转换的协议映射表：(前缀, 目标路径, 参数名)
        const PROTOCOL_MAPPINGS: &[(&str, &str, &str)] = &[
            ("entry://", "entry", "key"),
            ("entryx://", "entryx", "entry_no"),
            ("sound://", "sound", "key"),
            ("source://", "source", "entry_no"),
            ("file://", "mdd", "key"),
        ];
        
        // 检查转换映射表
        for (scheme, action, param_name) in PROTOCOL_MAPPINGS {
            if url.starts_with(scheme) {
                // 直接处理原始路径，避免URL库的双重编码
                let path_with_fragment = &url[scheme.len()..];
                
                // 分离路径和fragment
                let (path_part, fragment_part) = if let Some(hash_pos) = path_with_fragment.find('#') {
                    (&path_with_fragment[..hash_pos], Some(&path_with_fragment[hash_pos + 1..]))
                } else {
                    (path_with_fragment, None)
                };
                
                // 如果路径已经包含编码字符，先解码再重新编码，避免双重编码
                let decoded_path = if path_part.contains('%') {
                    // 尝试URL解码
                    percent_encoding::percent_decode_str(path_part)
                        .decode_utf8()
                        .unwrap_or_else(|_| path_part.into())
                        .to_string()
                } else {
                    path_part.to_string()
                };
                
                let clean_path = if action == &"mdd" || action == &"sound" {
                    // mdd类型添加前导斜杠
                    if decoded_path.starts_with('/') {
                        decoded_path
                    } else {
                        format!("/{}", decoded_path)
                    }
                } else {
                    // 非mdd类型去掉前导斜杠
                    decoded_path.trim_start_matches('/').to_string()
                };
                
                let base_url_trimmed = base_url.trim_end_matches('/');
                
                // 使用URL库构造结果URL，但直接传入原始路径让URL库只编码一次
                if let Ok(mut result_url) = Url::parse(&format!("{}/{}?profile_id={}", base_url_trimmed, action, profile_id)) {
                    result_url.query_pairs_mut()
                        .append_pair(param_name, &clean_path);
                    
                    // 设置fragment（URL库会自动编码）
                    if let Some(frag) = fragment_part {
                        result_url.set_fragment(Some(frag));
                    }
                    
                    return result_url.to_string();
                }
            }
        }
               
        // 检查前13个字符中是否包含":", 处理形如"mailto:","tel:","javascript:","data:"等协议
        let prefix = if url.len() > 13 { &url[..13] } else { url };
        if prefix.contains(':') {
            return url.to_string();
        }
        
        // 没有协议的相对路径，默认使用mdd
        // 构造file:///格式让URL库解析
        let file_url = if url.starts_with('/') {
            format!("file://{}", url)
        } else {
            format!("file:///{}", url)
        };
        
        if let Ok(parsed_url) = Url::parse(&file_url) {
            let path = parsed_url.path();
            let fragment = parsed_url.fragment();
            
            let base_url_trimmed = base_url.trim_end_matches('/');
            
            // 使用URL库构造结果
            if let Ok(mut result_url) = Url::parse(&format!("{}/mdd?profile_id={}", base_url_trimmed, profile_id)) {
                result_url.query_pairs_mut()
                    .append_pair("key", path);
                
                // 设置fragment
                if let Some(frag) = fragment {
                    result_url.set_fragment(Some(frag));
                }
                
                return result_url.to_string();
            }
        }
        
        // 如果所有URL库操作都失败，回退到原始URL
        url.to_string()
    }

    /// 重写CSS中的url()引用
    pub fn rewrite_css_urls(css: &str, profile_id: i32, base_url: &str) -> String {
        use regex::Regex;
        
        let url_regex = Regex::new(r#"url\s*\(\s*(['"]?)([^'")]+)(['"]?)\s*\)"#).unwrap();
        
        url_regex.replace_all(css, |caps: &regex::Captures| {
            let quote1 = &caps[1];
            let url = &caps[2];
            let quote2 = &caps[3];
            let new_url = Self::rewrite_url(url, profile_id, base_url);
            format!("url({}{}{})", quote1, new_url, quote2)
        }).to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;



    #[test]
    fn test_rewrite_url() {
        let base_url = "mdx://mdict.cn/service/";
        
        // 统一的测试用例数组：(输入URL, 期望输出)
        let test_cases = [
            // 基本协议转换 - entry和source类型去掉根路径"/"，sound和mdd类型保留根路径"/"
            ("entry://test.html", "mdx://mdict.cn/service/entry?profile_id=123&key=test.html"),
            ("entryx://test.html", "mdx://mdict.cn/service/entryx?profile_id=123&entry_no=test.html"),
            ("sound://test.mp3", "mdx://mdict.cn/service/sound?profile_id=123&key=%2Ftest.mp3"),
            ("source://test.txt", "mdx://mdict.cn/service/source?profile_id=123&entry_no=test.txt"),
            
            // 三斜杠格式处理
            ("entry:///test.html", "mdx://mdict.cn/service/entry?profile_id=123&key=test.html"),
            ("sound:///test.mp3", "mdx://mdict.cn/service/sound?profile_id=123&key=%2Ftest.mp3"),
            
            // mdd类型保留完整编码路径
            ("file://test.png", "mdx://mdict.cn/service/mdd?profile_id=123&key=%2Ftest.png"),
            ("file:///test.png", "mdx://mdict.cn/service/mdd?profile_id=123&key=%2Ftest.png"),
            
            // 相对路径默认为mdd类型
            ("test.png", "mdx://mdict.cn/service/mdd?profile_id=123&key=%2Ftest.png"),
            ("images/test.png", "mdx://mdict.cn/service/mdd?profile_id=123&key=%2Fimages%2Ftest.png"),
            
            // 包含未编码字符的URL
            ("entry://abc def.html", "mdx://mdict.cn/service/entry?profile_id=123&key=abc+def.html"),
            ("sound://audio file.mp3", "mdx://mdict.cn/service/sound?profile_id=123&key=%2Faudio+file.mp3"),
            ("file://path with spaces/file.png", "mdx://mdict.cn/service/mdd?profile_id=123&key=%2Fpath+with+spaces%2Ffile.png"),
            ("entry://测试文件.html", "mdx://mdict.cn/service/entry?profile_id=123&key=%E6%B5%8B%E8%AF%95%E6%96%87%E4%BB%B6.html"),
            
            // 包含特殊字符的URL
            ("entry://file{1}.html", "mdx://mdict.cn/service/entry?profile_id=123&key=file%7B1%7D.html"),
            ("sound://track<2>.mp3", "mdx://mdict.cn/service/sound?profile_id=123&key=%2Ftrack%3C2%3E.mp3"),
            ("file://path:with:colons.txt", "mdx://mdict.cn/service/mdd?profile_id=123&key=%2Fpath%3Awith%3Acolons.txt"),
            
            // 包含fragment的URL
            ("entry://page.html#section 1", "mdx://mdict.cn/service/entry?profile_id=123&key=page.html#section%201"),
            ("file://doc.pdf#第一章", "mdx://mdict.cn/service/mdd?profile_id=123&key=%2Fdoc.pdf#%E7%AC%AC%E4%B8%80%E7%AB%A0"),
            ("sound://audio.mp3#chapter{1}", "mdx://mdict.cn/service/sound?profile_id=123&key=%2Faudio.mp3#chapter{1}"),
            
            // 包含已编码字符的URL（应该先解码再重新编码）
            ("entry://hello%20world.html", "mdx://mdict.cn/service/entry?profile_id=123&key=hello+world.html"),
            ("file://%E6%B5%8B%E8%AF%95.png", "mdx://mdict.cn/service/mdd?profile_id=123&key=%2F%E6%B5%8B%E8%AF%95.png"),
            
            // 特殊处理：entry://#fragment 格式
            ("entry://#section1", "#section1"),
            ("entry:///#section2", "#section2"),
            ("entry://#锚点", "#锚点"),
            ("entry:///#anchor with spaces", "#anchor with spaces"),
            ("entry://#", "#"),
            
            // 不应该重写的URL
            ("http://example.com", "http://example.com"),
            ("https://example.com", "https://example.com"),
            ("#anchor", "#anchor"),
            ("mdx://mdict.cn/test", "mdx://mdict.cn/test"),
            ("mailto:test@example.com", "mailto:test@example.com"),
            ("tel:+1234567890", "tel:+1234567890"),
            ("javascript:alert('test')", "javascript:alert('test')"),
            ("data:image/png;base64,abc", "data:image/png;base64,abc"),
        ];
        
        for (input, expected) in test_cases {
            let result = MdxHtmlRewriter::rewrite_url(input, 123, base_url);
            assert_eq!(result, expected, "Failed for input: {}", input);
        }
    }

    #[test]
    fn test_rewrite_css_urls() {
        let base_url = "mdx://mdict.cn/service/";
        let css = "background: url('image.png'); background-image: url(\"file://test.jpg\");";
        let expected = "background: url('mdx://mdict.cn/service/mdd?profile_id=123&key=%2Fimage.png'); background-image: url(\"mdx://mdict.cn/service/mdd?profile_id=123&key=%2Ftest.jpg\");";
        assert_eq!(MdxHtmlRewriter::rewrite_css_urls(css, 123, base_url), expected);
    }



    #[test]
    fn test_custom_base_url() -> Result<()> {
        let html = r#"<img src="entry://test.png">"#;
        
        let result = MdxHtmlRewriter::rewrite_html_with_base_url(html, 123, "custom://example.com")?;
        assert!(result.contains("custom://example.com/entry?profile_id=123&key=test.png"));
        
        Ok(())
    }

    #[test] 
    fn test_multiple_attributes_in_single_element() -> Result<()> {
        let html = r#"<object data="file://data.swf" codebase="source://code/" classid="entry://class">content</object>"#;
        
        let result = MdxHtmlRewriter::rewrite_html(html, 123)?;
        assert!(result.contains("mdx://mdict.cn/service/mdd?profile_id=123&key=%2Fdata.swf"));
        assert!(result.contains("mdx://mdict.cn/service/source?profile_id=123&entry_no=code"));
        assert!(result.contains("mdx://mdict.cn/service/entry?profile_id=123&key=class"));
        
        Ok(())
    }

    #[test]
    fn test_rewrite_html() -> Result<()> {
        let html = r#"<img src="entry://test.png" alt="test"><a href="sound://test.mp3">link</a>"#;
        
        let result = MdxHtmlRewriter::rewrite_html(html, 123)?;
        
        assert!(result.contains("mdx://mdict.cn/service/entry?profile_id=123&key=test.png"));
        assert!(result.contains("mdx://mdict.cn/service/sound?profile_id=123&key=%2Ftest.mp3"));
        
        Ok(())
    }



    #[test]
    fn test_rewrite_html_custom_base_url() -> Result<()> {
        let html = r#"<img src="entry://test.png">"#;
        
        let result = MdxHtmlRewriter::rewrite_html_with_base_url(html, 123, "custom://example.com")?;
        assert!(result.contains("custom://example.com/entry?profile_id=123&key=test.png"));
        
        Ok(())
    }

    #[test]
    fn test_large_html_performance() -> Result<()> {
        // 创建一个大的HTML文档
        let mut html = String::with_capacity(100_000);
        html.push_str("<html><body>");
        for i in 0..1000 {
            html.push_str(&format!(
                r#"<div><img src="entry://image{}.png"><a href="sound://audio{}.mp3">Link {}</a></div>"#,
                i, i, i
            ));
        }
        html.push_str("</body></html>");
        
        // 测试HTML处理
        let result = MdxHtmlRewriter::rewrite_html(&html, 123)?;
        assert!(result.contains("mdx://mdict.cn/service/entry?profile_id=123&key=image0.png"));
        assert!(result.contains("mdx://mdict.cn/service/sound?profile_id=123&key=%2Faudio999.mp3"));
        
        Ok(())
    }





    #[test]
    fn test_html_rewrite_with_special_characters() -> Result<()> {
        // 测试HTML中包含特殊字符的链接重写
        let test_cases = [
            // HTML内容, 期望包含的URL片段
            (r#"<img src="entry://hello world.png">"#, 
             "mdx://mdict.cn/service/entry?profile_id=123&key=hello+world.png"),
            (r#"<a href="sound://音乐.mp3">音频</a>"#, 
             "mdx://mdict.cn/service/sound?profile_id=123&key=%2F%E9%9F%B3%E4%B9%90.mp3"),
            (r#"<img src="file://path with spaces/image.png">"#, 
             "mdx://mdict.cn/service/mdd?profile_id=123&key=%2Fpath+with+spaces%2Fimage.png"),
            (r#"<a href="entry://abc def#no 1">链接</a>"#, 
             "mdx://mdict.cn/service/entry?profile_id=123&key=abc+def#no%201"),
        ];
        
        for (html, expected_fragment) in test_cases {
            let result = MdxHtmlRewriter::rewrite_html(html, 123)?;
            assert!(result.contains(expected_fragment), 
                   "Failed for HTML: {}\nExpected fragment: {}\nActual result: {}", 
                   html, expected_fragment, result);
        }
        
        Ok(())
    }


    #[test]
    fn test_fragment_in_html() -> Result<()> {
        // 测试HTML中带fragment的链接重写
        let html = r#"<a href="entry://page.html#章节一">链接</a><img src="file://image.png#meta data">"#;
        
        let result = MdxHtmlRewriter::rewrite_html(html, 123)?;
        
        // 验证主URL部分和fragment部分都被正确编码
        assert!(result.contains("mdx://mdict.cn/service/entry?profile_id=123&key=page.html#%E7%AB%A0%E8%8A%82%E4%B8%80"));
        assert!(result.contains("mdx://mdict.cn/service/mdd?profile_id=123&key=%2Fimage.png#meta%20data"));
        
        Ok(())
    }

    #[test]
    fn test_fragment_only_links_preservation() -> Result<()> {
        // 测试纯fragment链接（#开头）应该原样保留
        let test_cases = [
            "#section1",
            "#chapter-2", 
            "#锚点",
            "#anchor with spaces",
            "#",
            "#123",
        ];
        
        for url in test_cases {
            let result = MdxHtmlRewriter::rewrite_url(url, 123, "mdx://mdict.cn/service/");
            assert_eq!(url, result, "Fragment-only link '{}' should remain unchanged", url);
        }
        
        // 测试HTML中的fragment链接
        let html = "<a href=\"#section1\">Go to section 1</a><a href=\"#锚点\">Go to anchor</a>";
        let result = MdxHtmlRewriter::rewrite_html(html, 123)?;
        
        assert!(result.contains("href=\"#section1\""), "Fragment link #section1 should be preserved in HTML");
        assert!(result.contains("href=\"#锚点\""), "Fragment link #锚点 should be preserved in HTML");
        
        Ok(())
    }

    #[test]
    fn test_entry_fragment_special_handling() -> Result<()> {
        // 测试 entry://#fragment 和 entry:///#fragment 的特殊处理
        let test_cases = [
            ("entry://#section1", "#section1"),
            ("entry:///#section2", "#section2"),
            ("entry://#锚点", "#锚点"),
            ("entry:///#anchor with spaces", "#anchor with spaces"),
            ("entry://#", "#"),
            ("entry:///#", "#"),
        ];
        
        for (input, expected) in test_cases {
            let result = MdxHtmlRewriter::rewrite_url(input, 123, "mdx://mdict.cn/service/");
            assert_eq!(result, expected, "entry:// fragment link '{}' should be converted to '{}'", input, expected);
        }
        
        // 测试HTML中的 entry:// fragment链接
        let html = "<a href=\"entry://#section1\">Go to section 1</a><a href=\"entry:///#锚点\">Go to anchor</a>";
        let result = MdxHtmlRewriter::rewrite_html(html, 123)?;
        
        assert!(result.contains("href=\"#section1\""), "entry:// fragment link should be converted to pure fragment in HTML");
        assert!(result.contains("href=\"#锚点\""), "entry:// fragment link with Chinese characters should be converted correctly");
        
        // 确保正常的entry://链接仍然正常转换
        let normal_entry_html = "<a href=\"entry://page.html\">Normal entry link</a>";
        let normal_result = MdxHtmlRewriter::rewrite_html(normal_entry_html, 123)?;
        assert!(normal_result.contains("mdx://mdict.cn/service/entry?profile_id=123&key=page.html"), 
                "Normal entry:// links should still be converted normally");
        
        Ok(())
    }





}