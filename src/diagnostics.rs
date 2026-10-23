use serde::Serialize;

use crate::lexer::Span;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Error,
    Warning,
    Info,
}

impl Severity {
    pub fn as_str(self) -> &'static str {
        match self {
            Severity::Error => "error",
            Severity::Warning => "warning",
            Severity::Info => "info",
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Diagnostic {
    pub severity: Severity,
    /// Stable machine-readable rule id, e.g. `syntax`, `unresolved-reference`.
    pub code: String,
    pub message: String,
    /// Component (task / data item / function) the issue belongs to, if any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub component: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub col: Option<u32>,
}

impl Diagnostic {
    pub fn new(severity: Severity, code: &str, message: impl Into<String>) -> Self {
        Diagnostic {
            severity,
            code: code.to_string(),
            message: message.into(),
            component: None,
            line: None,
            col: None,
        }
    }

    pub fn error(code: &str, message: impl Into<String>) -> Self {
        Self::new(Severity::Error, code, message)
    }

    pub fn warning(code: &str, message: impl Into<String>) -> Self {
        Self::new(Severity::Warning, code, message)
    }

    pub fn info(code: &str, message: impl Into<String>) -> Self {
        Self::new(Severity::Info, code, message)
    }

    pub fn at(mut self, span: Span) -> Self {
        if span.line > 0 {
            self.line = Some(span.line);
            self.col = Some(span.col);
        }
        self
    }

    pub fn in_component(mut self, name: impl Into<String>) -> Self {
        self.component = Some(name.into());
        self
    }

    pub fn render(&self, file: &str) -> String {
        let loc = match (self.line, self.col) {
            (Some(l), Some(c)) => format!("{}:{}:{}", file, l, c),
            _ => file.to_string(),
        };
        let comp = self
            .component
            .as_deref()
            .map(|c| format!(" [{}]", c))
            .unwrap_or_default();
        format!(
            "{}: {}[{}]{}: {}",
            loc,
            self.severity.as_str(),
            self.code,
            comp,
            self.message
        )
    }
}

impl Diagnostic {
    /// Errors that mean the parse tree is incomplete (lexical/syntax/grammar-action
    /// failures). Editing such a file would silently drop content.
    pub fn is_fatal(&self) -> bool {
        matches!(
            self.code.as_str(),
            "PF-L001" | "PF-L002" | "PF-L003" | "PF-L004" | "PF-L005"
        )
    }
}
