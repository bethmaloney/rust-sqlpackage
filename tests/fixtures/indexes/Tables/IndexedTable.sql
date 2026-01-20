CREATE TABLE [dbo].[IndexedTable] (
    [Id] INT NOT NULL,
    [Category] NVARCHAR(50) NOT NULL,
    [Name] NVARCHAR(100) NOT NULL,
    [Description] NVARCHAR(500) NULL,
    [CreatedAt] DATETIME2 NOT NULL,
    CONSTRAINT [PK_IndexedTable] PRIMARY KEY NONCLUSTERED ([Id])
);
