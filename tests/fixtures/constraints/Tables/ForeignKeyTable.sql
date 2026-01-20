CREATE TABLE [dbo].[ForeignKeyTable] (
    [Id] INT NOT NULL PRIMARY KEY,
    [ParentId] INT NOT NULL,
    CONSTRAINT [FK_ForeignKeyTable_Parent] FOREIGN KEY ([ParentId])
        REFERENCES [dbo].[PrimaryKeyTable]([Id])
);
