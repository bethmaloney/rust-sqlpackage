-- FK with ON DELETE CASCADE and ON UPDATE CASCADE
CREATE TABLE [dbo].[ChildCascade] (
    [Id] INT NOT NULL,
    [ParentId] INT NOT NULL,
    [Data] NVARCHAR(200) NULL,
    CONSTRAINT [PK_ChildCascade] PRIMARY KEY ([Id]),
    CONSTRAINT [FK_ChildCascade_Parent] FOREIGN KEY ([ParentId])
        REFERENCES [dbo].[Parent]([Id])
        ON DELETE CASCADE
        ON UPDATE CASCADE
);
GO
