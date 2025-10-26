//! Database entities

pub mod captured_request;
pub mod captured_tcp_connection;

pub use captured_request::Entity as CapturedRequest;
pub use captured_tcp_connection::Entity as CapturedTcpConnection;

pub mod prelude {
    pub use super::captured_request::Entity as CapturedRequest;
    pub use super::captured_tcp_connection::Entity as CapturedTcpConnection;
}
