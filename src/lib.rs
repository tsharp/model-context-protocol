//! # Model Context Protocol
//!
//! A Rust implementation of the Model Context Protocol (MCP).
//!
//! This library provides types and traits for building MCP servers and clients.
//!
//! ## Example
//!
//! ```rust
//! use model_context_protocol::Protocol;
//!
//! // Example usage will be added as the library develops
//! ```

use serde::{Deserialize, Serialize};

/// The current version of the MCP protocol
pub const PROTOCOL_VERSION: &str = "0.1.0";

/// Represents the Model Context Protocol interface
pub trait Protocol {
    /// Initialize the protocol connection
    fn initialize(&mut self) -> Result<(), Error>;
    
    /// Send a message through the protocol
    fn send_message(&self, message: Message) -> Result<(), Error>;
    
    /// Receive a message from the protocol
    fn receive_message(&self) -> Result<Message, Error>;
}

/// A message in the Model Context Protocol
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    /// Message ID
    pub id: String,
    /// Message type
    pub message_type: MessageType,
    /// Message payload
    pub payload: serde_json::Value,
}

/// Types of messages in the protocol
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MessageType {
    /// Request message
    Request,
    /// Response message
    Response,
    /// Notification message
    Notification,
    /// Error message
    Error,
}

/// Error types for the protocol
#[derive(Debug, Clone)]
pub enum Error {
    /// Connection error
    ConnectionError(String),
    /// Serialization error
    SerializationError(String),
    /// Protocol error
    ProtocolError(String),
    /// Unknown error
    Unknown(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::ConnectionError(msg) => write!(f, "Connection error: {}", msg),
            Error::SerializationError(msg) => write!(f, "Serialization error: {}", msg),
            Error::ProtocolError(msg) => write!(f, "Protocol error: {}", msg),
            Error::Unknown(msg) => write!(f, "Unknown error: {}", msg),
        }
    }
}

impl std::error::Error for Error {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_protocol_version() {
        assert_eq!(PROTOCOL_VERSION, "0.1.0");
    }

    #[test]
    fn test_message_serialization() {
        let msg = Message {
            id: "test-123".to_string(),
            message_type: MessageType::Request,
            payload: serde_json::json!({"test": "data"}),
        };
        
        let serialized = serde_json::to_string(&msg).unwrap();
        let deserialized: Message = serde_json::from_str(&serialized).unwrap();
        
        assert_eq!(msg.id, deserialized.id);
    }
}
