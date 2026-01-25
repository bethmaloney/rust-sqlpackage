-- Table with inline constraints that need SqlInlineConstraintAnnotation
-- Tests inline UNIQUE, CHECK, and DEFAULT on column definitions
CREATE TABLE [dbo].[InlineConstraintDemo] (
    [Id] INT NOT NULL IDENTITY(1, 1) PRIMARY KEY,
    [Email] NVARCHAR(255) NOT NULL UNIQUE,
    [Phone] NVARCHAR(20) NULL,
    [Balance] DECIMAL(18, 2) NOT NULL DEFAULT 0.00 CHECK ([Balance] >= -1000),
    [CreditLimit] DECIMAL(18, 2) NOT NULL DEFAULT 1000.00,
    [Status] NVARCHAR(20) NOT NULL DEFAULT 'Active' CHECK ([Status] IN ('Active', 'Suspended', 'Closed')),
    [Age] INT NULL CHECK ([Age] >= 0 AND [Age] <= 150),
    [Score] DECIMAL(5, 2) NOT NULL DEFAULT 0.00 CHECK ([Score] >= 0 AND [Score] <= 100)
);
GO
