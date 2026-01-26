-- Table with ALL constraint types: PK, FK, UQ, CK, DF
CREATE TABLE [dbo].[Products] (
    [Id] INT NOT NULL,
    [SKU] NVARCHAR(50) NOT NULL,
    [Name] NVARCHAR(200) NOT NULL,
    [CategoryId] INT NOT NULL,
    [Price] DECIMAL(18,2) NOT NULL,
    [Quantity] INT NOT NULL,
    [IsActive] BIT NOT NULL CONSTRAINT [DF_Products_IsActive] DEFAULT (1),
    [CreatedAt] DATETIME NOT NULL CONSTRAINT [DF_Products_CreatedAt] DEFAULT (GETDATE()),

    -- PK: Primary Key Constraint
    CONSTRAINT [PK_Products] PRIMARY KEY ([Id]),

    -- FK: Foreign Key Constraint
    CONSTRAINT [FK_Products_Categories] FOREIGN KEY ([CategoryId]) REFERENCES [dbo].[Categories]([Id]),

    -- UQ: Unique Constraint
    CONSTRAINT [UQ_Products_SKU] UNIQUE ([SKU]),

    -- CK: Check Constraint
    CONSTRAINT [CK_Products_Price] CHECK ([Price] >= 0),
    CONSTRAINT [CK_Products_Quantity] CHECK ([Quantity] >= 0)
);
GO
