CREATE TABLE [dbo].[LargeTable] (
    [Id] INT NOT NULL,
    [Category] INT NOT NULL,
    [Name] NVARCHAR(200) NOT NULL,
    [Description] NVARCHAR(MAX) NULL,
    [CreatedAt] DATETIME NOT NULL,
    [ModifiedAt] DATETIME NULL,
    [IsActive] BIT NOT NULL DEFAULT 1,
    CONSTRAINT [PK_LargeTable] PRIMARY KEY ([Id])
);
GO
