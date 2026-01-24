-- Table with constraint added WITH CHECK (validates existing data - default)
CREATE TABLE [dbo].[ValidatedConstraints] (
    [Id] INT NOT NULL,
    [ParentId] INT NOT NULL,
    [Amount] DECIMAL(18,2) NOT NULL,
    CONSTRAINT [PK_ValidatedConstraints] PRIMARY KEY ([Id])
);
GO

-- Add FK constraint WITH CHECK (explicit - validates existing rows)
ALTER TABLE [dbo].[ValidatedConstraints] WITH CHECK
ADD CONSTRAINT [FK_ValidatedConstraints_Parent] FOREIGN KEY ([ParentId])
    REFERENCES [dbo].[Parent]([Id]);
GO

-- Add CHECK constraint WITH CHECK
ALTER TABLE [dbo].[ValidatedConstraints] WITH CHECK
ADD CONSTRAINT [CK_ValidatedConstraints_Amount] CHECK ([Amount] >= 0);
GO
