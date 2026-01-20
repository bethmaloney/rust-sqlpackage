CREATE TABLE [dbo].[UniqueConstraintTable] (
    [Id] INT NOT NULL PRIMARY KEY,
    [Email] NVARCHAR(255) NOT NULL,
    [Username] NVARCHAR(100) NOT NULL,
    CONSTRAINT [UQ_UniqueConstraintTable_Email] UNIQUE ([Email]),
    CONSTRAINT [UQ_UniqueConstraintTable_Username] UNIQUE ([Username])
);
