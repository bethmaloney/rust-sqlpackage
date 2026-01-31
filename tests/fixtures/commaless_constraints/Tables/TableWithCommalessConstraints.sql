-- Table with constraints that lack comma separators before them
-- This pattern is accepted by SQL Server but may not be parsed correctly
CREATE TABLE [dbo].[TableWithCommalessConstraints]
(
    [Id] UNIQUEIDENTIFIER NOT NULL,
    [Version] INT NOT NULL DEFAULT ((0)),
    [CreatedOn] DATETIME NOT NULL,
    [ModifiedOn] DATETIME NOT NULL,
    [Name] NVARCHAR(100) NOT NULL
    CONSTRAINT [PK_TableWithCommalessConstraints] PRIMARY KEY CLUSTERED ([Id] ASC),
    CONSTRAINT [FK_TableWithCommalessConstraints_Self] FOREIGN KEY ([Id]) REFERENCES [dbo].[TableWithCommalessConstraints] ([Id])
);
GO
