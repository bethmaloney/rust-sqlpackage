CREATE TABLE [dbo].[Products] (
    [Id] INT NOT NULL,
    [CategoryId] INT NOT NULL,
    [Name] NVARCHAR(100) NOT NULL,
    [Price] DECIMAL(18,2) NOT NULL,
    [IsActive] BIT NOT NULL DEFAULT 1,
    CONSTRAINT [PK_Products] PRIMARY KEY ([Id])
);
GO
