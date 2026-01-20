CREATE TABLE [dbo].[Products] (
    [Id] INT NOT NULL PRIMARY KEY,
    [SKU] NVARCHAR(50) NOT NULL,
    [Name] NVARCHAR(200) NOT NULL,
    [CategoryId] INT NOT NULL,
    [Price] DECIMAL(18, 2) NOT NULL,
    [Quantity] INT NOT NULL,
    CONSTRAINT [FK_Products_Categories] FOREIGN KEY ([CategoryId]) REFERENCES [dbo].[Categories]([Id])
);
GO
