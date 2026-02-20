/// LSP method name constants used when communicating with Tinymist.
pub mod text_document {
    pub const DID_OPEN: &str = "textDocument/didOpen";
    pub const DID_CHANGE: &str = "textDocument/didChange";
    pub const DID_SAVE: &str = "textDocument/didSave";
    pub const FORMATTING: &str = "textDocument/formatting";
    pub const HOVER: &str = "textDocument/hover";
    pub const COMPLETION: &str = "textDocument/completion";
    pub const DOCUMENT_SYMBOL: &str = "textDocument/documentSymbol";
    pub const PUBLISH_DIAGNOSTICS: &str = "textDocument/publishDiagnostics";
}

pub mod window {
    pub const SHOW_MESSAGE: &str = "window/showMessage";
    pub const LOG_MESSAGE: &str = "window/logMessage";
}
