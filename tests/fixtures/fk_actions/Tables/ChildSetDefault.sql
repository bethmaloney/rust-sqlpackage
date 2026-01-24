-- FK with ON DELETE SET DEFAULT and ON UPDATE SET DEFAULT
CREATE TABLE [dbo].[ChildSetDefault] (
    [Id] INT NOT NULL,
    [ParentId] INT NOT NULL CONSTRAINT [DF_ChildSetDefault_ParentId] DEFAULT (1),
    [Data] NVARCHAR(200) NULL,
    CONSTRAINT [PK_ChildSetDefault] PRIMARY KEY ([Id]),
    CONSTRAINT [FK_ChildSetDefault_Parent] FOREIGN KEY ([ParentId])
        REFERENCES [dbo].[Parent]([Id])
        ON DELETE SET DEFAULT
        ON UPDATE SET DEFAULT
);
GO
