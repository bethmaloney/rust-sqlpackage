CREATE TABLE [dbo].[Products] (
    [Id] INT NOT NULL,
    [SKU] VARCHAR(50) NOT NULL,
    [Name] NVARCHAR(200) NOT NULL,
    [Category] NVARCHAR(100) NOT NULL,
    [Price] DECIMAL(18, 2) NOT NULL,
    [Description] NVARCHAR(MAX) NULL,
    [CreatedAt] DATETIME2 NOT NULL,
    CONSTRAINT [PK_Products] PRIMARY KEY NONCLUSTERED ([Id])
);
