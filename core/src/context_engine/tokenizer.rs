use std::collections::HashSet;

const STOPWORDS: &[&str] = &[
    // English
    "a", "about", "after", "all", "also", "an", "and", "any", "are", "as", "at", "be", "because",
    "been", "before", "being", "but", "by", "can", "could", "did", "do", "does", "for", "from",
    "had", "has", "have", "he", "her", "here", "him", "his", "how", "i", "if", "in", "into", "is",
    "it", "its", "just", "me", "more", "most", "my", "no", "not", "of", "on", "only", "or",
    "other", "our", "out", "over", "same", "she", "should", "so", "some", "such", "than", "that",
    "the", "their", "them", "then", "there", "these", "they", "this", "those", "to", "too",
    "under", "up", "us", "very", "was", "we", "were", "what", "when", "where", "which", "who",
    "why", "will", "with", "would", "you", "your", // Spanish
    "al", "algo", "algunas", "algunos", "ante", "antes", "como", "con", "contra", "cual", "cuales",
    "cuando", "de", "del", "desde", "donde", "dos", "el", "ella", "ellas", "ellos", "en", "entre",
    "era", "es", "esa", "ese", "eso", "esta", "estas", "este", "esto", "estos", "fue", "ha", "han",
    "hasta", "hay", "la", "las", "le", "les", "lo", "los", "mas", "menos", "mi", "mis", "mucho",
    "muy", "nos", "o", "otra", "otras", "otro", "otros", "para", "pero", "por", "porque", "que",
    "quien", "quienes", "se", "ser", "si", "sin", "sobre", "son", "su", "sus", "tambien", "te",
    "tiene", "todo", "todos", "tu", "tus", "un", "una", "unas", "uno", "unos", "y", "ya",
];

fn is_stopword(token: &str) -> bool {
    STOPWORDS.contains(&token)
}

pub struct Tokenizer;

impl Tokenizer {
    pub fn tokenize(text: &str) -> Vec<String> {
        let text = text.to_lowercase();
        let mut tokens = Vec::new();
        let mut current_token = String::new();

        for ch in text.chars() {
            if ch.is_alphanumeric() || ch == '_' {
                current_token.push(ch);
            } else if !current_token.is_empty() {
                tokens.push(current_token.clone());
                current_token.clear();
            }
        }

        if !current_token.is_empty() {
            tokens.push(current_token);
        }

        let mut tokens = Self::split_compound_tokens(tokens);
        tokens.retain(|token| !is_stopword(token));
        tokens
    }

    fn split_compound_tokens(tokens: Vec<String>) -> Vec<String> {
        let mut result = Vec::new();

        for token in tokens {
            let mut split_tokens = Self::split_camel_case(&token);
            split_tokens.extend(Self::split_snake_case(&token));

            if split_tokens.is_empty() {
                result.push(token);
            } else {
                result.extend(split_tokens);
            }
        }

        result
    }

    fn split_camel_case(token: &str) -> Vec<String> {
        let mut result = Vec::new();
        let mut current = String::new();
        let mut prev_was_lower = false;

        for ch in token.chars() {
            if ch.is_uppercase() && prev_was_lower && !current.is_empty() {
                if current.len() > 1 {
                    result.push(current.clone());
                }
                current.clear();
            }
            current.push(ch);
            prev_was_lower = ch.is_lowercase();
        }

        if !current.is_empty() && current.len() > 1 {
            result.push(current);
        }

        result
    }

    fn split_snake_case(token: &str) -> Vec<String> {
        if token.contains('_') {
            token
                .split('_')
                .filter(|s| !s.is_empty())
                .map(|s| s.to_string())
                .collect()
        } else {
            Vec::new()
        }
    }

    pub fn remove_punctuation(tokens: Vec<String>) -> Vec<String> {
        tokens
            .into_iter()
            .map(|t| {
                t.chars()
                    .filter(|c| c.is_alphanumeric() || *c == '_')
                    .collect()
            })
            .filter(|s: &String| !s.is_empty())
            .collect()
    }

    pub fn tokenize_with_frequency(text: &str) -> HashSet<(String, usize)> {
        let tokens = Self::tokenize(text);
        let mut freq: HashSet<(String, usize)> = HashSet::new();

        for token in tokens {
            let count = freq
                .iter()
                .find(|(t, _)| *t == token)
                .map(|(_, c)| *c)
                .unwrap_or(0);
            freq.insert((token, count + 1));
        }

        freq
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_tokenization() {
        let tokens = Tokenizer::tokenize("hello world");
        assert_eq!(tokens, vec!["hello", "world"]);
    }

    #[test]
    fn test_split_camel_case() {
        let tokens = Tokenizer::tokenize("myFunctionName");
        let has_function = tokens
            .iter()
            .any(|t| t.contains("function") || t.contains("name"));
        assert!(has_function, "Tokens: {:?}", tokens);
    }

    #[test]
    fn test_split_snake_case() {
        let tokens = Tokenizer::tokenize("my_function_name");
        assert!(!tokens.contains(&"my".to_string()));
        assert!(tokens.contains(&"function".to_string()));
        assert!(tokens.contains(&"name".to_string()));
    }

    #[test]
    fn test_filters_spanish_and_english_stopwords() {
        let tokens = Tokenizer::tokenize("de que trata este proyecto and the main function");
        assert_eq!(tokens, vec!["trata", "proyecto", "main", "function"]);
    }

    #[test]
    fn test_remove_punctuation() {
        let tokens = vec!["hello,".to_string(), "world!".to_string()];
        let cleaned = Tokenizer::remove_punctuation(tokens);
        assert!(cleaned.contains(&"hello".to_string()));
        assert!(cleaned.contains(&"world".to_string()));
    }
}
