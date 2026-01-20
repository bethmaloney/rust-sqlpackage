CREATE TABLE [dbo].[Products] (
    [Id] INT NOT NULL PRIMARY KEY,
    [Name] NVARCHAR(200) NOT NULL,
    [Category] NVARCHAR(100) NOT NULL,
    [Price] DECIMAL(18,2) NOT NULL,
    [CreatedAt] DATETIME NOT NULL
);
GO
