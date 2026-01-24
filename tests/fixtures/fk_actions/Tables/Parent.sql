-- Parent table for FK action tests
CREATE TABLE [dbo].[Parent] (
    [Id] INT NOT NULL,
    [Name] NVARCHAR(100) NOT NULL,
    CONSTRAINT [PK_Parent] PRIMARY KEY ([Id])
);
GO
