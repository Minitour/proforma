//! PROforma guideline language toolkit: parser, semantic model, editing,
//! linting, serialization and diagram export, shared by the CLI and the MCP server.

pub mod ast;
pub mod builtins;
pub mod diagnostics;
pub mod edit;
pub mod lexer;
pub mod lint;
pub mod mermaid;
pub mod model;
pub mod parser;
pub mod reference;
pub mod tools;
pub mod writer;
