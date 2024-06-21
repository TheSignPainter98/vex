use std::ops::Deref;

use tree_sitter::Query as TSQuery;

use crate::{
    error::Error, result::Result, suggestion::suggest, supported_language::SupportedLanguage,
};

#[derive(Debug)]
pub struct Query(TSQuery);

impl Query {
    pub const KNOWN_OPERATORS: [&'static str; 8] = [
        "#eq?",
        "#match?",
        "#any-of?",
        "#not-eq?",
        "#not-match?",
        "#not-any-of?",
        "#any-eq?",
        "#any-match?",
    ];

    pub fn new(language: SupportedLanguage, query: &str) -> Result<Self> {
        let query = TSQuery::new(language.ts_language(), query)?;

        if query.pattern_count() == 0 {
            return Err(Error::EmptyQuery);
        }

        for pattern_index in 0..query.pattern_count() {
            if let Some(predicate) = query.general_predicates(pattern_index).first() {
                let operator = predicate.operator.to_string();
                let suggestion = suggest(&operator, Self::KNOWN_OPERATORS);
                return Err(Error::UnknownOperator {
                    operator,
                    suggestion,
                });
            }
        }

        Ok(Self(query))
    }
}

impl Deref for Query {
    type Target = TSQuery;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
