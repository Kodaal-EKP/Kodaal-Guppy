use std::collections::HashSet;

const SUGGESTION_CANDIDATE_LIMIT: i64 = 80;
const SUGGESTION_QUERY_MAX_CHARS: usize = 10_000;
const SUGGESTION_STOP_WORDS: [&str; 34] = [
    "about", "after", "also", "and", "are", "because", "but", "can", "could", "for", "from",
    "has", "have", "how", "into", "like", "make", "need", "not", "now", "please", "should",
    "that", "the", "this", "through", "use", "using", "want", "what", "when", "where",
    "with", "would",
];

impl Database {
    pub fn suggest_prompts(
        &self,
        query: SuggestionQuery,
        config: &crate::config::SuggestionsConfig,
    ) -> Result<PromptSuggestionList, ApiError> {
        let surface = query.surface.trim();
        if !matches!(surface, "cli" | "ide") {
            return Err(ApiError::invalid_query(
                "surface must be cli or ide",
                Some("surface"),
            ));
        }
        let enabled = config.enabled
            && ((surface == "cli" && config.cli_enabled)
                || (surface == "ide" && config.ide_enabled));
        let limit = query.limit.unwrap_or(config.limit as i64);
        if !(1..=10).contains(&limit) {
            return Err(ApiError::invalid_query("limit must be 1-10", Some("limit")));
        }
        let draft = query.q.trim();
        if draft.chars().count() > SUGGESTION_QUERY_MAX_CHARS {
            return Err(ApiError::invalid_query(
                "q max length is 10000 characters",
                Some("q"),
            ));
        }
        if !enabled || draft.chars().count() < config.min_chars as usize {
            return Ok(PromptSuggestionList {
                enabled,
                surface: surface.to_string(),
                similar_count: 0,
                min_chars: config.min_chars,
                items: Vec::new(),
            });
        }

        let query_terms = suggestion_terms(draft);
        if query_terms.is_empty() {
            return Ok(PromptSuggestionList {
                enabled,
                surface: surface.to_string(),
                similar_count: 0,
                min_chars: config.min_chars,
                items: Vec::new(),
            });
        }

        let match_query = query_terms
            .iter()
            .map(|term| format!("{term}*"))
            .collect::<Vec<_>>()
            .join(" OR ");
        let mut clauses = vec!["prompts_fts MATCH ?", "p.source = ?"];
        let mut values = vec![
            SqlValue::Text(match_query),
            SqlValue::Text(surface.to_string()),
        ];
        if let Some(source_app) = query.source_app.as_deref().map(str::trim).filter(|v| !v.is_empty()) {
            validate_source_app_filter(source_app)?;
            clauses.push("p.source_app = ?");
            values.push(SqlValue::Text(source_app.to_string()));
        }
        if let Some(project_id) = query.project_id.as_deref().map(str::trim).filter(|v| !v.is_empty()) {
            clauses.push("p.project_id = ?");
            values.push(SqlValue::Text(project_id.to_string()));
        }
        values.push(SqlValue::Integer(SUGGESTION_CANDIDATE_LIMIT));

        let sql = format!(
            "SELECT p.id, p.text, p.source, p.source_app, p.project_id, pr.name, p.conversation_id, p.conversation_title, p.use_count, p.favorite, p.metadata, p.created_at, p.last_used_at, p.redacted, p.redaction_reason
             FROM prompts p JOIN prompts_fts f ON f.rowid = p.rowid LEFT JOIN projects pr ON pr.id = p.project_id
             WHERE {} ORDER BY p.use_count DESC, p.last_used_at DESC LIMIT ?",
            clauses.join(" AND ")
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(params_from_iter(values), |row| self.prompt_from_row(row))?;
        let mut ranked = Vec::new();
        for row in rows {
            let mut prompt = row?;
            let candidate_terms = suggestion_terms(&prompt.text);
            let matched_terms = query_terms
                .iter()
                .filter(|term| candidate_terms.contains(*term))
                .cloned()
                .collect::<Vec<_>>();
            if matched_terms.is_empty() {
                continue;
            }
            let required_terms = if query_terms.len() >= 4 { 2 } else { 1 };
            if matched_terms.len() < required_terms {
                continue;
            }
            let score = matched_terms.len() as f64 / query_terms.len() as f64;
            prompt.tags = self.tags_for_prompt(&prompt.id)?;
            ranked.push(PromptSuggestion {
                id: prompt.id,
                text: prompt.text,
                source: prompt.source,
                source_app: prompt.source_app,
                project_id: prompt.project_id,
                project_name: prompt.project_name,
                use_count: prompt.use_count,
                favorite: prompt.favorite,
                tags: prompt.tags,
                score: (score * 1000.0).round() / 1000.0,
                matched_terms,
                created_at: prompt.created_at,
                last_used_at: prompt.last_used_at,
            });
        }
        ranked.sort_by(|left, right| {
            right
                .score
                .partial_cmp(&left.score)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| right.use_count.cmp(&left.use_count))
                .then_with(|| right.last_used_at.cmp(&left.last_used_at))
        });
        let similar_count = i64::try_from(ranked.len()).unwrap_or(i64::MAX);
        ranked.truncate(limit as usize);
        Ok(PromptSuggestionList {
            enabled,
            surface: surface.to_string(),
            similar_count,
            min_chars: config.min_chars,
            items: ranked,
        })
    }
}

fn suggestion_terms(text: &str) -> Vec<String> {
    let mut terms = Vec::new();
    let mut seen = HashSet::new();
    let mut current = String::new();
    for ch in text.chars() {
        if ch.is_ascii_alphanumeric() {
            current.push(ch.to_ascii_lowercase());
        } else {
            push_suggestion_term(&mut terms, &mut seen, &mut current);
        }
    }
    push_suggestion_term(&mut terms, &mut seen, &mut current);
    terms
}

fn push_suggestion_term(terms: &mut Vec<String>, seen: &mut HashSet<String>, current: &mut String) {
    if current.len() >= 3 && !SUGGESTION_STOP_WORDS.contains(&current.as_str()) {
        let term = current.clone();
        if seen.insert(term.clone()) {
            terms.push(term);
        }
    }
    current.clear();
}

fn validate_source_app_filter(value: &str) -> Result<(), ApiError> {
    if value.len() > 64 {
        return Err(ApiError::invalid_query(
            "source_app max length is 64",
            Some("source_app"),
        ));
    }
    if !value
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | '-'))
    {
        return Err(ApiError::invalid_query(
            "source_app may contain only letters, digits, dot, underscore, and dash",
            Some("source_app"),
        ));
    }
    Ok(())
}
