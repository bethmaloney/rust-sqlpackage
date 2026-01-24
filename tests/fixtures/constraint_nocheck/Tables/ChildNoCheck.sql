-- Table with constraint added WITH NOCHECK (for existing data)
CREATE TABLE [dbo].[ChildNoCheck] (
    [Id] INT NOT NULL,
    [ParentId] INT NOT NULL,
    [Value] INT NOT NULL,
    CONSTRAINT [PK_ChildNoCheck] PRIMARY KEY ([Id])
);
GO

-- Add FK constraint WITH NOCHECK (skips validation of existing rows)
ALTER TABLE [dbo].[ChildNoCheck] WITH NOCHECK
ADD CONSTRAINT [FK_ChildNoCheck_Parent] FOREIGN KEY ([ParentId])
    REFERENCES [dbo].[Parent]([Id]);
GO

-- Add CHECK constraint WITH NOCHECK
ALTER TABLE [dbo].[ChildNoCheck] WITH NOCHECK
ADD CONSTRAINT [CK_ChildNoCheck_Value] CHECK ([Value] > 0);
GO
