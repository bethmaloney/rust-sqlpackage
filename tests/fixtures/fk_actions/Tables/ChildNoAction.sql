-- FK with ON DELETE NO ACTION and ON UPDATE NO ACTION (explicit)
CREATE TABLE [dbo].[ChildNoAction] (
    [Id] INT NOT NULL,
    [ParentId] INT NOT NULL,
    [Data] NVARCHAR(200) NULL,
    CONSTRAINT [PK_ChildNoAction] PRIMARY KEY ([Id]),
    CONSTRAINT [FK_ChildNoAction_Parent] FOREIGN KEY ([ParentId])
        REFERENCES [dbo].[Parent]([Id])
        ON DELETE NO ACTION
        ON UPDATE NO ACTION
);
GO
