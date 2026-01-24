-- User-defined scalar type for email addresses (allows NULL)
CREATE TYPE [dbo].[EmailAddress] FROM NVARCHAR(255) NULL;
GO
