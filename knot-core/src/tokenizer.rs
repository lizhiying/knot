use jieba_rs::{Jieba, Token as JiebaToken, TokenizeMode};
use std::sync::Arc;
use tantivy::tokenizer::{Token, TokenStream, Tokenizer};

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
    type TokenStream<'a> = JiebaTokenStream<'a>;

    fn token_stream<'a>(&'a mut self, text: &'a str) -> Self::TokenStream<'a> {
        let tokens = self.jieba.tokenize(text, TokenizeMode::Search, true);
        JiebaTokenStream {
            tokens,
            index: 0,
            token: Token::default(),
        }
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
}
