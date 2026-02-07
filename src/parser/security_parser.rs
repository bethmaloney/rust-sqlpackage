//! Token-based parser for security statements (CREATE USER, CREATE ROLE,
//! ALTER ROLE ADD/DROP MEMBER, GRANT/DENY/REVOKE).

use sqlparser::keywords::Keyword;
use sqlparser::tokenizer::Token;

use super::token_parser_base::TokenParser;

/// Authentication type for CREATE USER
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UserAuthType {
    /// FOR LOGIN [login_name]
    Login(String),
    /// WITHOUT LOGIN
    WithoutLogin,
    /// FROM EXTERNAL PROVIDER
    ExternalProvider,
    /// No explicit auth clause (defaults to login-based)
    Default,
}

/// Parsed CREATE USER result
#[derive(Debug, Clone)]
pub struct TokenParsedUser {
    pub name: String,
    pub auth_type: UserAuthType,
    pub default_schema: Option<String>,
}

/// Parsed CREATE ROLE result
#[derive(Debug, Clone)]
pub struct TokenParsedRole {
    pub name: String,
    pub owner: Option<String>,
}

/// Parsed ALTER ROLE ... ADD/DROP MEMBER result
#[derive(Debug, Clone)]
pub struct TokenParsedRoleMembership {
    pub role: String,
    pub member: String,
    pub is_add: bool,
}

/// Permission action type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PermissionAction {
    Grant,
    Deny,
    Revoke,
}

/// Target of a permission (what it's granted ON)
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PermissionTarget {
    /// ON [schema].[object] or ON [object]
    Object {
        schema: Option<String>,
        name: String,
    },
    /// ON SCHEMA::[schema_name]
    Schema(String),
    /// Database-level (no ON clause)
    Database,
}

/// Parsed GRANT/DENY/REVOKE result
#[derive(Debug, Clone)]
pub struct TokenParsedPermission {
    pub action: PermissionAction,
    pub permission: String,
    pub target: PermissionTarget,
    pub principal: String,
    pub with_grant_option: bool,
    pub cascade: bool,
}

pub struct SecurityTokenParser {
    base: TokenParser,
}

impl SecurityTokenParser {
    pub fn new(sql: &str) -> Option<Self> {
        Some(Self {
            base: TokenParser::new(sql)?,
        })
    }

    /// Parse CREATE USER [name] with various options
    pub fn parse_create_user(&mut self) -> Option<TokenParsedUser> {
        self.base.skip_whitespace();

        // Expect CREATE
        if !self.base.check_keyword(Keyword::CREATE) {
            return None;
        }
        self.base.advance();
        self.base.skip_whitespace();

        // Expect USER
        if !self.base.check_word_ci("USER") {
            return None;
        }
        self.base.advance();
        self.base.skip_whitespace();

        // Parse user name
        let name = self.base.parse_identifier()?;
        self.base.skip_whitespace();

        let mut auth_type = UserAuthType::Default;
        let mut default_schema = None;

        // Parse optional clauses
        loop {
            if self.base.is_at_end() {
                break;
            }

            // FOR LOGIN [login_name]
            if self.base.check_keyword(Keyword::FOR) {
                self.base.advance();
                self.base.skip_whitespace();
                if self.base.check_word_ci("LOGIN") {
                    self.base.advance();
                    self.base.skip_whitespace();
                    let login = self.base.parse_identifier()?;
                    auth_type = UserAuthType::Login(login);
                    self.base.skip_whitespace();
                    continue;
                }
            }

            // WITHOUT LOGIN
            if self.base.check_word_ci("WITHOUT") {
                self.base.advance();
                self.base.skip_whitespace();
                if self.base.check_word_ci("LOGIN") {
                    self.base.advance();
                    self.base.skip_whitespace();
                    auth_type = UserAuthType::WithoutLogin;
                    continue;
                }
            }

            // FROM EXTERNAL PROVIDER
            if self.base.check_keyword(Keyword::FROM) {
                self.base.advance();
                self.base.skip_whitespace();
                if self.base.check_word_ci("EXTERNAL") {
                    self.base.advance();
                    self.base.skip_whitespace();
                    if self.base.check_word_ci("PROVIDER") {
                        self.base.advance();
                        self.base.skip_whitespace();
                        auth_type = UserAuthType::ExternalProvider;
                        continue;
                    }
                }
            }

            // WITH DEFAULT_SCHEMA = [schema]
            if self.base.check_keyword(Keyword::WITH) {
                self.base.advance();
                self.base.skip_whitespace();
                if self.base.check_word_ci("DEFAULT_SCHEMA") {
                    self.base.advance();
                    self.base.skip_whitespace();
                    // Skip '='
                    if self.base.check_token(&Token::Eq) {
                        self.base.advance();
                        self.base.skip_whitespace();
                    }
                    default_schema = Some(self.base.parse_identifier()?);
                    self.base.skip_whitespace();
                    continue;
                }
            }

            // Skip unknown tokens
            self.base.advance();
        }

        Some(TokenParsedUser {
            name,
            auth_type,
            default_schema,
        })
    }

    /// Parse CREATE ROLE [name] with optional AUTHORIZATION [owner]
    pub fn parse_create_role(&mut self) -> Option<TokenParsedRole> {
        self.base.skip_whitespace();

        // Expect CREATE
        if !self.base.check_keyword(Keyword::CREATE) {
            return None;
        }
        self.base.advance();
        self.base.skip_whitespace();

        // Expect ROLE
        if !self.base.check_word_ci("ROLE") {
            return None;
        }
        self.base.advance();
        self.base.skip_whitespace();

        // Parse role name
        let name = self.base.parse_identifier()?;
        self.base.skip_whitespace();

        let mut owner = None;

        // Check for AUTHORIZATION clause
        if self.base.check_word_ci("AUTHORIZATION") {
            self.base.advance();
            self.base.skip_whitespace();
            owner = Some(self.base.parse_identifier()?);
        }

        Some(TokenParsedRole { name, owner })
    }

    /// Parse ALTER ROLE [role] ADD MEMBER [member] or DROP MEMBER [member]
    pub fn parse_alter_role_membership(&mut self) -> Option<TokenParsedRoleMembership> {
        self.base.skip_whitespace();

        // Expect ALTER
        if !self.base.check_keyword(Keyword::ALTER) {
            return None;
        }
        self.base.advance();
        self.base.skip_whitespace();

        // Expect ROLE
        if !self.base.check_word_ci("ROLE") {
            return None;
        }
        self.base.advance();
        self.base.skip_whitespace();

        // Parse role name
        let role = self.base.parse_identifier()?;
        self.base.skip_whitespace();

        // Expect ADD or DROP
        let is_add = if self.base.check_keyword(Keyword::ADD) {
            self.base.advance();
            self.base.skip_whitespace();
            true
        } else if self.base.check_keyword(Keyword::DROP) {
            self.base.advance();
            self.base.skip_whitespace();
            false
        } else {
            return None;
        };

        // Expect MEMBER
        if !self.base.check_word_ci("MEMBER") {
            return None;
        }
        self.base.advance();
        self.base.skip_whitespace();

        // Parse member name
        let member = self.base.parse_identifier()?;

        Some(TokenParsedRoleMembership {
            role,
            member,
            is_add,
        })
    }

    /// Parse GRANT/DENY/REVOKE permission statements
    pub fn parse_permission(&mut self) -> Option<TokenParsedPermission> {
        self.base.skip_whitespace();

        // Determine action
        let action = if self.base.check_keyword(Keyword::GRANT) {
            self.base.advance();
            PermissionAction::Grant
        } else if self.base.check_word_ci("DENY") {
            self.base.advance();
            PermissionAction::Deny
        } else if self.base.check_word_ci("REVOKE") {
            self.base.advance();
            PermissionAction::Revoke
        } else {
            return None;
        };
        self.base.skip_whitespace();

        // For REVOKE, skip optional GRANT OPTION FOR
        if action == PermissionAction::Revoke && self.base.check_keyword(Keyword::GRANT) {
            self.base.advance();
            self.base.skip_whitespace();
            // OPTION
            if self.base.check_word_ci("OPTION") {
                self.base.advance();
                self.base.skip_whitespace();
                // FOR
                if self.base.check_keyword(Keyword::FOR) {
                    self.base.advance();
                    self.base.skip_whitespace();
                }
            }
        }

        // Parse permission name (may be multi-word like "SELECT", "EXECUTE", "ALTER ANY SCHEMA", "VIEW DEFINITION")
        let permission = self.parse_permission_name()?;
        self.base.skip_whitespace();

        // Parse optional ON clause
        let target = if self.base.check_keyword(Keyword::ON) {
            self.base.advance();
            self.base.skip_whitespace();
            self.parse_permission_target()?
        } else {
            PermissionTarget::Database
        };

        self.base.skip_whitespace();

        // Expect TO or FROM
        if self.base.check_keyword(Keyword::TO) || self.base.check_keyword(Keyword::FROM) {
            self.base.advance();
            self.base.skip_whitespace();
        } else {
            return None;
        }

        // Parse principal name
        let principal = self.base.parse_identifier()?;
        self.base.skip_whitespace();

        // Check for WITH GRANT OPTION
        let mut with_grant_option = false;
        let mut cascade = false;

        while !self.base.is_at_end() {
            if self.base.check_keyword(Keyword::WITH) {
                self.base.advance();
                self.base.skip_whitespace();
                if self.base.check_keyword(Keyword::GRANT) {
                    self.base.advance();
                    self.base.skip_whitespace();
                    if self.base.check_word_ci("OPTION") {
                        self.base.advance();
                        self.base.skip_whitespace();
                        with_grant_option = true;
                        continue;
                    }
                }
            }

            if self.base.check_keyword(Keyword::CASCADE) {
                self.base.advance();
                self.base.skip_whitespace();
                cascade = true;
                continue;
            }

            break;
        }

        Some(TokenParsedPermission {
            action,
            permission,
            target,
            principal,
            with_grant_option,
            cascade,
        })
    }

    /// Parse a permission name (may be multi-word like "VIEW DEFINITION", "ALTER ANY SCHEMA")
    fn parse_permission_name(&mut self) -> Option<String> {
        let mut parts = Vec::new();

        // Collect words that are part of the permission name
        // Stop at keywords that indicate end of permission name: ON, TO, FROM
        loop {
            if self.base.is_at_end() {
                break;
            }

            // Stop at ON, TO, FROM keywords
            if self.base.check_keyword(Keyword::ON)
                || self.base.check_keyword(Keyword::TO)
                || self.base.check_keyword(Keyword::FROM)
            {
                break;
            }

            // Collect the word
            if let Some(token) = self.base.current_token() {
                match &token.token {
                    Token::Word(w) => {
                        parts.push(w.value.to_uppercase());
                        self.base.advance();
                        self.base.skip_whitespace();
                    }
                    Token::Comma => {
                        // Multiple permissions (e.g., GRANT SELECT, INSERT ON ...)
                        // For now we only capture the first permission
                        break;
                    }
                    _ => break,
                }
            } else {
                break;
            }
        }

        if parts.is_empty() {
            None
        } else {
            Some(parts.join(" "))
        }
    }

    /// Parse the target of a permission (after ON keyword)
    fn parse_permission_target(&mut self) -> Option<PermissionTarget> {
        // Check for SCHEMA:: prefix
        if self.base.check_word_ci("SCHEMA") {
            self.base.advance();
            self.base.skip_whitespace();
            // Expect :: (two colons)
            if self.base.check_token(&Token::DoubleColon) {
                self.base.advance();
                self.base.skip_whitespace();
            }
            let schema_name = self.base.parse_identifier()?;
            return Some(PermissionTarget::Schema(schema_name));
        }

        // Check for OBJECT:: prefix (explicit)
        if self.base.check_word_ci("OBJECT") {
            self.base.advance();
            self.base.skip_whitespace();
            if self.base.check_token(&Token::DoubleColon) {
                self.base.advance();
                self.base.skip_whitespace();
            }
        }

        // Parse object reference: [schema].[name] or [name]
        let first = self.base.parse_identifier()?;
        self.base.skip_whitespace();

        if self.base.check_token(&Token::Period) {
            self.base.advance();
            self.base.skip_whitespace();
            let second = self.base.parse_identifier()?;
            Some(PermissionTarget::Object {
                schema: Some(first),
                name: second,
            })
        } else {
            Some(PermissionTarget::Object {
                schema: None,
                name: first,
            })
        }
    }
}

/// Parse sp_addrolemember 'role', 'member'
pub fn parse_sp_addrolemember(sql: &str) -> Option<TokenParsedRoleMembership> {
    let mut parser = TokenParser::new(sql)?;
    parser.skip_whitespace();

    // Skip EXEC/EXECUTE if present
    if parser.check_keyword(Keyword::EXECUTE) || parser.check_word_ci("EXEC") {
        parser.advance();
        parser.skip_whitespace();
    }

    // Expect sp_addrolemember or sp_droprolemember
    let is_add = if parser.check_word_ci("sp_addrolemember") {
        true
    } else if parser.check_word_ci("sp_droprolemember") {
        false
    } else {
        return None;
    };
    parser.advance();
    parser.skip_whitespace();

    // Parse role name (string literal or identifier)
    let role = parse_string_or_ident(&mut parser)?;
    parser.skip_whitespace();

    // Skip comma
    if parser.check_token(&Token::Comma) {
        parser.advance();
        parser.skip_whitespace();
    }

    // Parse member name (string literal or identifier)
    let member = parse_string_or_ident(&mut parser)?;

    Some(TokenParsedRoleMembership {
        role,
        member,
        is_add,
    })
}

/// Parse a string literal or identifier from the token stream
fn parse_string_or_ident(parser: &mut TokenParser) -> Option<String> {
    if let Some(token) = parser.current_token() {
        match &token.token {
            Token::SingleQuotedString(s) => {
                let val = s.clone();
                parser.advance();
                Some(val)
            }
            Token::NationalStringLiteral(s) => {
                let val = s.clone();
                parser.advance();
                Some(val)
            }
            _ => parser.parse_identifier(),
        }
    } else {
        None
    }
}

/// Top-level convenience function to parse CREATE USER
pub fn parse_create_user_tokens(sql: &str) -> Option<TokenParsedUser> {
    let mut parser = SecurityTokenParser::new(sql)?;
    parser.parse_create_user()
}

/// Top-level convenience function to parse CREATE ROLE
pub fn parse_create_role_tokens(sql: &str) -> Option<TokenParsedRole> {
    let mut parser = SecurityTokenParser::new(sql)?;
    parser.parse_create_role()
}

/// Top-level convenience function to parse ALTER ROLE ... ADD/DROP MEMBER
pub fn parse_alter_role_membership_tokens(sql: &str) -> Option<TokenParsedRoleMembership> {
    let mut parser = SecurityTokenParser::new(sql)?;
    parser.parse_alter_role_membership()
}

/// Top-level convenience function to parse GRANT/DENY/REVOKE
pub fn parse_permission_tokens(sql: &str) -> Option<TokenParsedPermission> {
    let mut parser = SecurityTokenParser::new(sql)?;
    parser.parse_permission()
}

#[cfg(test)]
mod tests {
    use super::*;

    // ===== CREATE USER tests =====

    #[test]
    fn test_parse_create_user_for_login() {
        let result = parse_create_user_tokens("CREATE USER [AppUser] FOR LOGIN [AppLogin]");
        let user = result.expect("Should parse CREATE USER FOR LOGIN");
        assert_eq!(user.name, "AppUser");
        assert_eq!(user.auth_type, UserAuthType::Login("AppLogin".to_string()));
        assert_eq!(user.default_schema, None);
    }

    #[test]
    fn test_parse_create_user_without_login() {
        let result = parse_create_user_tokens("CREATE USER [ContainedUser] WITHOUT LOGIN");
        let user = result.expect("Should parse CREATE USER WITHOUT LOGIN");
        assert_eq!(user.name, "ContainedUser");
        assert_eq!(user.auth_type, UserAuthType::WithoutLogin);
    }

    #[test]
    fn test_parse_create_user_with_default_schema() {
        let result = parse_create_user_tokens(
            "CREATE USER [AppUser] FOR LOGIN [AppLogin] WITH DEFAULT_SCHEMA = [app]",
        );
        let user = result.expect("Should parse CREATE USER with DEFAULT_SCHEMA");
        assert_eq!(user.name, "AppUser");
        assert_eq!(user.auth_type, UserAuthType::Login("AppLogin".to_string()));
        assert_eq!(user.default_schema, Some("app".to_string()));
    }

    #[test]
    fn test_parse_create_user_from_external_provider() {
        let result = parse_create_user_tokens("CREATE USER [AzureADUser] FROM EXTERNAL PROVIDER");
        let user = result.expect("Should parse CREATE USER FROM EXTERNAL PROVIDER");
        assert_eq!(user.name, "AzureADUser");
        assert_eq!(user.auth_type, UserAuthType::ExternalProvider);
    }

    #[test]
    fn test_parse_create_user_without_login_with_schema() {
        let result = parse_create_user_tokens(
            "CREATE USER [ContainedUser] WITHOUT LOGIN WITH DEFAULT_SCHEMA = [dbo]",
        );
        let user = result.expect("Should parse");
        assert_eq!(user.name, "ContainedUser");
        assert_eq!(user.auth_type, UserAuthType::WithoutLogin);
        assert_eq!(user.default_schema, Some("dbo".to_string()));
    }

    #[test]
    fn test_parse_create_user_simple() {
        let result = parse_create_user_tokens("CREATE USER [SimpleUser]");
        let user = result.expect("Should parse simple CREATE USER");
        assert_eq!(user.name, "SimpleUser");
        assert_eq!(user.auth_type, UserAuthType::Default);
    }

    // ===== CREATE ROLE tests =====

    #[test]
    fn test_parse_create_role_simple() {
        let result = parse_create_role_tokens("CREATE ROLE [AppRole]");
        let role = result.expect("Should parse CREATE ROLE");
        assert_eq!(role.name, "AppRole");
        assert_eq!(role.owner, None);
    }

    #[test]
    fn test_parse_create_role_with_authorization() {
        let result = parse_create_role_tokens("CREATE ROLE [AppRole] AUTHORIZATION [dbo]");
        let role = result.expect("Should parse CREATE ROLE with AUTHORIZATION");
        assert_eq!(role.name, "AppRole");
        assert_eq!(role.owner, Some("dbo".to_string()));
    }

    // ===== ALTER ROLE MEMBERSHIP tests =====

    #[test]
    fn test_parse_alter_role_add_member() {
        let result =
            parse_alter_role_membership_tokens("ALTER ROLE [db_datareader] ADD MEMBER [AppUser]");
        let membership = result.expect("Should parse ALTER ROLE ADD MEMBER");
        assert_eq!(membership.role, "db_datareader");
        assert_eq!(membership.member, "AppUser");
        assert!(membership.is_add);
    }

    #[test]
    fn test_parse_alter_role_drop_member() {
        let result =
            parse_alter_role_membership_tokens("ALTER ROLE [db_datawriter] DROP MEMBER [AppUser]");
        let membership = result.expect("Should parse ALTER ROLE DROP MEMBER");
        assert_eq!(membership.role, "db_datawriter");
        assert_eq!(membership.member, "AppUser");
        assert!(!membership.is_add);
    }

    #[test]
    fn test_parse_sp_addrolemember() {
        let result = parse_sp_addrolemember("EXEC sp_addrolemember 'db_datareader', 'AppUser'");
        let membership = result.expect("Should parse sp_addrolemember");
        assert_eq!(membership.role, "db_datareader");
        assert_eq!(membership.member, "AppUser");
        assert!(membership.is_add);
    }

    #[test]
    fn test_parse_sp_droprolemember() {
        let result = parse_sp_addrolemember("EXEC sp_droprolemember 'db_datawriter', 'AppUser'");
        let membership = result.expect("Should parse sp_droprolemember");
        assert_eq!(membership.role, "db_datawriter");
        assert_eq!(membership.member, "AppUser");
        assert!(!membership.is_add);
    }

    // ===== PERMISSION tests =====

    #[test]
    fn test_parse_grant_select_on_table() {
        let result = parse_permission_tokens("GRANT SELECT ON [dbo].[Employees] TO [AppRole]");
        let perm = result.expect("Should parse GRANT SELECT");
        assert_eq!(perm.action, PermissionAction::Grant);
        assert_eq!(perm.permission, "SELECT");
        assert_eq!(
            perm.target,
            PermissionTarget::Object {
                schema: Some("dbo".to_string()),
                name: "Employees".to_string()
            }
        );
        assert_eq!(perm.principal, "AppRole");
        assert!(!perm.with_grant_option);
    }

    #[test]
    fn test_parse_deny_delete_on_table() {
        let result = parse_permission_tokens("DENY DELETE ON [dbo].[AuditLog] TO [AppRole]");
        let perm = result.expect("Should parse DENY DELETE");
        assert_eq!(perm.action, PermissionAction::Deny);
        assert_eq!(perm.permission, "DELETE");
        assert_eq!(
            perm.target,
            PermissionTarget::Object {
                schema: Some("dbo".to_string()),
                name: "AuditLog".to_string()
            }
        );
        assert_eq!(perm.principal, "AppRole");
    }

    #[test]
    fn test_parse_revoke_execute_on_proc() {
        let result =
            parse_permission_tokens("REVOKE EXECUTE ON [dbo].[usp_GetData] FROM [AppRole]");
        let perm = result.expect("Should parse REVOKE EXECUTE");
        assert_eq!(perm.action, PermissionAction::Revoke);
        assert_eq!(perm.permission, "EXECUTE");
        assert_eq!(
            perm.target,
            PermissionTarget::Object {
                schema: Some("dbo".to_string()),
                name: "usp_GetData".to_string()
            }
        );
        assert_eq!(perm.principal, "AppRole");
    }

    #[test]
    fn test_parse_grant_on_schema() {
        let result = parse_permission_tokens("GRANT SELECT ON SCHEMA::[app] TO [AppRole]");
        let perm = result.expect("Should parse GRANT ON SCHEMA");
        assert_eq!(perm.action, PermissionAction::Grant);
        assert_eq!(perm.permission, "SELECT");
        assert_eq!(perm.target, PermissionTarget::Schema("app".to_string()));
        assert_eq!(perm.principal, "AppRole");
    }

    #[test]
    fn test_parse_grant_database_level() {
        let result = parse_permission_tokens("GRANT VIEW DEFINITION TO [AppRole]");
        let perm = result.expect("Should parse database-level GRANT");
        assert_eq!(perm.action, PermissionAction::Grant);
        assert_eq!(perm.permission, "VIEW DEFINITION");
        assert_eq!(perm.target, PermissionTarget::Database);
        assert_eq!(perm.principal, "AppRole");
    }

    #[test]
    fn test_parse_grant_with_grant_option() {
        let result = parse_permission_tokens(
            "GRANT SELECT ON [dbo].[Employees] TO [AppRole] WITH GRANT OPTION",
        );
        let perm = result.expect("Should parse GRANT WITH GRANT OPTION");
        assert_eq!(perm.action, PermissionAction::Grant);
        assert!(perm.with_grant_option);
    }

    #[test]
    fn test_parse_revoke_cascade() {
        let result =
            parse_permission_tokens("REVOKE SELECT ON [dbo].[Employees] FROM [AppRole] CASCADE");
        let perm = result.expect("Should parse REVOKE CASCADE");
        assert_eq!(perm.action, PermissionAction::Revoke);
        assert!(perm.cascade);
    }

    #[test]
    fn test_parse_grant_execute_on_object() {
        let result =
            parse_permission_tokens("GRANT EXECUTE ON OBJECT::[dbo].[usp_Insert] TO [AppRole]");
        let perm = result.expect("Should parse GRANT ON OBJECT::");
        assert_eq!(perm.permission, "EXECUTE");
        assert_eq!(
            perm.target,
            PermissionTarget::Object {
                schema: Some("dbo".to_string()),
                name: "usp_Insert".to_string()
            }
        );
    }
}
