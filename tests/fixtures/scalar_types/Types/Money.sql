-- User-defined scalar type for currency amounts
CREATE TYPE [dbo].[Currency] FROM DECIMAL(18,4) NOT NULL;
GO
