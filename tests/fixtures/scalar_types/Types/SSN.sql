-- User-defined scalar type for social security numbers (fixed length)
CREATE TYPE [dbo].[SSN] FROM CHAR(11) NOT NULL;
GO
