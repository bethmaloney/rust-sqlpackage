-- Table with multiple constraints lacking commas between them
CREATE TABLE [dbo].[TableWithMultipleCommalessConstraints]
(
    [Id] UNIQUEIDENTIFIER NOT NULL,
    [Version] INT CONSTRAINT [DF_MultiCommaless_Version] DEFAULT ((0)) NOT NULL,
    [CreatedOn] DATETIME CONSTRAINT [DF_MultiCommaless_CreatedOn] DEFAULT (GETDATE()) NOT NULL,
    [ParentId] UNIQUEIDENTIFIER NOT NULL,
    [Status] NVARCHAR(20) CONSTRAINT [DF_MultiCommaless_Status] DEFAULT ('Active') NOT NULL

    CONSTRAINT [PK_MultiCommaless] PRIMARY KEY ([Id] ASC)
    CONSTRAINT [FK_MultiCommaless_Parent] FOREIGN KEY ([ParentId]) REFERENCES [dbo].[TableWithCommalessConstraints] ([Id]),
    CONSTRAINT [CK_MultiCommaless_Status] CHECK ([Status] IN ('Active', 'Inactive', 'Pending'))
);
GO
