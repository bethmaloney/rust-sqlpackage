-- FK with ON DELETE SET NULL and ON UPDATE SET NULL
CREATE TABLE [dbo].[ChildSetNull] (
    [Id] INT NOT NULL,
    [ParentId] INT NULL,
    [Data] NVARCHAR(200) NULL,
    CONSTRAINT [PK_ChildSetNull] PRIMARY KEY ([Id]),
    CONSTRAINT [FK_ChildSetNull_Parent] FOREIGN KEY ([ParentId])
        REFERENCES [dbo].[Parent]([Id])
        ON DELETE SET NULL
        ON UPDATE SET NULL
);
GO
