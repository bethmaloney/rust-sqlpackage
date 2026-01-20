-- Parent table for FK reference
CREATE TABLE [dbo].[Categories] (
    [Id] INT NOT NULL,
    [Name] NVARCHAR(100) NOT NULL,
    CONSTRAINT [PK_Categories] PRIMARY KEY ([Id])
);
GO
