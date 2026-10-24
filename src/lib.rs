//! PROforma guideline language toolkit: parser, semantic model, editing,
//! linting, serialization and diagram export, shared by the CLI and the MCP server.

pub mod ast;
pub mod builtins;
pub mod diagnostics;
pub mod edit;
pub mod encoding;
pub mod export;
pub mod lexer;
pub mod lint;
pub mod mermaid;
pub mod model;
pub mod parser;
pub mod props;
pub mod reference;
pub mod tools;
pub mod writer;
