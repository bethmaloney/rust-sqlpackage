-- Users
CREATE USER [AppUser] FOR LOGIN [AppLogin];
GO

CREATE USER [ContainedUser] WITHOUT LOGIN;
GO

CREATE USER [ExternalUser] FROM EXTERNAL PROVIDER;
GO

-- Roles
CREATE ROLE [AppRole];
GO

CREATE ROLE [AdminRole] AUTHORIZATION [dbo];
GO

-- Role Membership
ALTER ROLE [db_datareader] ADD MEMBER [AppUser];
GO

ALTER ROLE [AppRole] ADD MEMBER [ContainedUser];
GO

-- Permissions
GRANT SELECT ON [dbo].[Employees] TO [AppRole];
GO

GRANT EXECUTE ON [dbo].[usp_GetEmployees] TO [AppRole];
GO

DENY DELETE ON [dbo].[AuditLog] TO [AppRole];
GO

GRANT SELECT ON SCHEMA::[dbo] TO [AdminRole];
GO

GRANT VIEW DEFINITION TO [AdminRole];
GO
