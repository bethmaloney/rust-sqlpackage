-- Table with inline constraints that need SqlInlineConstraintAnnotation
-- This tests whether columns are linked to their inline constraints
CREATE TABLE [dbo].[Customer] (
    [Id] INT NOT NULL IDENTITY(1,1),
    [Email] NVARCHAR(255) NOT NULL UNIQUE,
    [Phone] NVARCHAR(20) NULL,
    [Balance] DECIMAL(18, 2) NOT NULL DEFAULT 0.00,
    [CreditLimit] DECIMAL(18, 2) NOT NULL DEFAULT 1000.00,
    [IsActive] BIT NOT NULL DEFAULT 1,
    CONSTRAINT [PK_Customer] PRIMARY KEY ([Id])
);
GO

-- Table with inline CHECK constraint
CREATE TABLE [dbo].[Employee] (
    [Id] INT NOT NULL PRIMARY KEY,
    [Name] NVARCHAR(100) NOT NULL,
    [Age] INT NOT NULL CHECK ([Age] >= 18 AND [Age] <= 120),
    [Salary] DECIMAL(18, 2) NOT NULL CHECK ([Salary] > 0),
    [StartDate] DATE NOT NULL DEFAULT GETDATE()
);
GO

-- Table with multiple inline constraints per column
CREATE TABLE [dbo].[Account] (
    [Id] UNIQUEIDENTIFIER NOT NULL DEFAULT NEWID(),
    [Balance] DECIMAL(18, 2) NOT NULL DEFAULT 0.00 CHECK ([Balance] >= 0),
    [OverdraftLimit] DECIMAL(18, 2) NOT NULL DEFAULT 0.00,
    [Status] NVARCHAR(20) NOT NULL DEFAULT 'Active' CHECK ([Status] IN ('Active', 'Suspended', 'Closed')),
    CONSTRAINT [PK_Account] PRIMARY KEY ([Id])
);
GO
