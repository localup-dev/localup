//! Database entities

pub mod auth_token;
pub mod captured_request;
pub mod captured_tcp_connection;
pub mod custom_domain;
pub mod device_authorization;
pub mod domain_challenge;
pub mod magic_link_token;
pub mod team;
pub mod team_member;
pub mod user;

pub use auth_token::Entity as AuthToken;
pub use captured_request::Entity as CapturedRequest;
pub use captured_tcp_connection::Entity as CapturedTcpConnection;
pub use custom_domain::Entity as CustomDomain;
pub use device_authorization::Entity as DeviceAuthorization;
pub use domain_challenge::Entity as DomainChallenge;
pub use magic_link_token::Entity as MagicLinkToken;
pub use team::Entity as Team;
pub use team_member::Entity as TeamMember;
pub use user::Entity as User;

pub mod prelude {
    pub use super::auth_token::Entity as AuthToken;
    pub use super::captured_request::Entity as CapturedRequest;
    pub use super::captured_tcp_connection::Entity as CapturedTcpConnection;
    pub use super::custom_domain::Entity as CustomDomain;
    pub use super::device_authorization::Entity as DeviceAuthorization;
    pub use super::domain_challenge::Entity as DomainChallenge;
    pub use super::magic_link_token::Entity as MagicLinkToken;
    pub use super::team::Entity as Team;
    pub use super::team_member::Entity as TeamMember;
    pub use super::user::Entity as User;
}
