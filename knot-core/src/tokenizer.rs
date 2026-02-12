use jieba_rs::{Jieba, Token as JiebaToken, TokenizeMode};
use std::sync::Arc;
use tantivy::tokenizer::{BoxTokenStream, Token, TokenStream, Tokenizer};

#[derive(Clone)]
pub struct JiebaTokenizer {
    jieba: Arc<Jieba>,
}

impl JiebaTokenizer {
    pub fn new() -> Self {
        JiebaTokenizer {
            jieba: Arc::new(Jieba::new()),
        }
    }
}

impl Default for JiebaTokenizer {
    fn default() -> Self {
        JiebaTokenizer::new()
    }
}

pub struct JiebaTokenStream<'a> {
    tokens: Vec<JiebaToken<'a>>,
    index: usize,
    token: Token,
}

impl<'a> TokenStream for JiebaTokenStream<'a> {
    fn advance(&mut self) -> bool {
        if self.index < self.tokens.len() {
            let j_token = &self.tokens[self.index];
            self.token.text.clear();
            self.token.text.push_str(j_token.word);
            self.token.offset_from = j_token.start;
            self.token.offset_to = j_token.end;
            self.token.position = self.token.position.wrapping_add(1);

            self.index += 1;
            true
        } else {
            false
        }
    }

    fn token(&self) -> &Token {
        &self.token
    }

    fn token_mut(&mut self) -> &mut Token {
        &mut self.token
    }
}

impl Tokenizer for JiebaTokenizer {
    type TokenStream<'a> = BoxTokenStream<'a>;

    fn token_stream<'a>(&'a mut self, text: &'a str) -> BoxTokenStream<'a> {
        let tokens = self.jieba.tokenize(text, TokenizeMode::Search, true);
        let stream = JiebaTokenStream {
            tokens,
            index: 0,
            token: Token::default(),
        };
        BoxTokenStream::new(stream)
    }
}

// ============================================================
// ICU Tokenizer: 泛语言分词，使用 Unicode 边界切分
// ============================================================

/// ICU 分词器：基于 icu_segmenter 的 WordSegmenter 进行 Unicode 标准分词。
/// 适用于所有 Unicode 文本，不依赖特定语言词典。
#[derive(Clone, Debug, Default)]
pub struct ICUTokenizer;

pub struct ICUTokenStream {
    /// 预计算的 (word, offset_from, offset_to)
    segments: Vec<(String, usize, usize)>,
    index: usize,
    token: Token,
}

impl TokenStream for ICUTokenStream {
    fn advance(&mut self) -> bool {
        if self.index < self.segments.len() {
            let (ref word, from, to) = self.segments[self.index];
            self.token.text.clear();
            self.token.text.push_str(word);
            self.token.offset_from = from;
            self.token.offset_to = to;
            self.token.position = self.token.position.wrapping_add(1);
            self.index += 1;
            true
        } else {
            false
        }
    }

    fn token(&self) -> &Token {
        &self.token
    }

    fn token_mut(&mut self) -> &mut Token {
        &mut self.token
    }
}

impl Tokenizer for ICUTokenizer {
    type TokenStream<'a> = BoxTokenStream<'a>;

    fn token_stream<'a>(&'a mut self, text: &'a str) -> BoxTokenStream<'a> {
        use icu_segmenter::WordSegmenter;

        let segmenter = WordSegmenter::new_auto();
        let breakpoints: Vec<usize> = segmenter.segment_str(text).collect();

        let mut segments = Vec::new();
        for window in breakpoints.windows(2) {
            let start = window[0];
            let end = window[1];
            let word = &text[start..end];

            // 跳过纯空白和纯标点
            let trimmed = word.trim();
            if trimmed.is_empty() {
                continue;
            }
            // 跳过仅由标点/符号组成的片段
            if trimmed
                .chars()
                .all(|c| c.is_ascii_punctuation() || !c.is_alphanumeric())
            {
                continue;
            }

            segments.push((word.to_string(), start, end));
        }

        let stream = ICUTokenStream {
            segments,
            index: 0,
            token: Token::default(),
        };
        BoxTokenStream::new(stream)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_jieba_mixed_language() {
        let mut tokenizer = JiebaTokenizer::new();
        let text = "什么是vanna";
        let mut stream = tokenizer.token_stream(text);

        let mut tokens = Vec::new();
        while stream.advance() {
            tokens.push(stream.token().text.clone());
        }

        // Expected: ["什么", "是", "vanna"] or similar segmentation depending on dict.
        // "vanna" should be preserved as a token.
        assert!(tokens.contains(&"vanna".to_string()));
        assert!(tokens.contains(&"什么".to_string()));
    }

    #[test]
    fn test_jieba_english_only() {
        let mut tokenizer = JiebaTokenizer::new();
        let text = "hello world";
        let mut stream = tokenizer.token_stream(text);

        let mut tokens = Vec::new();
        while stream.advance() {
            tokens.push(stream.token().text.clone());
        }

        assert!(tokens.contains(&"hello".to_string()));
        assert!(tokens.contains(&"world".to_string()));
    }

    #[test]
    fn test_icu_multilingual() {
        let mut tokenizer = ICUTokenizer;
        let text = "Hello世界test";
        let mut stream = tokenizer.token_stream(text);

        let mut tokens = Vec::new();
        while stream.advance() {
            tokens.push(stream.token().text.clone());
        }

        println!("ICU tokens for '{}': {:?}", text, tokens);
        // ICU 应该能切分中英混合文本
        assert!(!tokens.is_empty());
    }

    #[test]
    fn test_icu_japanese() {
        let mut tokenizer = ICUTokenizer;
        let text = "東京は日本の首都です";
        let mut stream = tokenizer.token_stream(text);

        let mut tokens = Vec::new();
        while stream.advance() {
            tokens.push(stream.token().text.clone());
        }

        println!("ICU tokens for '{}': {:?}", text, tokens);
        assert!(!tokens.is_empty());
    }

    #[test]
    fn test_icu_english() {
        let mut tokenizer = ICUTokenizer;
        let text = "Running tests for models";
        let mut stream = tokenizer.token_stream(text);

        let mut tokens = Vec::new();
        while stream.advance() {
            tokens.push(stream.token().text.clone());
        }

        println!("ICU tokens for '{}': {:?}", text, tokens);
        assert!(tokens.contains(&"Running".to_string()));
        assert!(tokens.contains(&"tests".to_string()));
        assert!(tokens.contains(&"models".to_string()));
    }
}
