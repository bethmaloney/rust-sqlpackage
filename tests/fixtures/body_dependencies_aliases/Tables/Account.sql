-- Base table for testing alias references
CREATE TABLE [dbo].[Account]
(
    [Id] INT NOT NULL PRIMARY KEY,
    [AccountNumber] NVARCHAR(50) NOT NULL,
    [InstrumentId] INT NULL,
    [Status] NVARCHAR(20) NOT NULL,
    [CreatedOn] DATETIME NOT NULL,
    [ModifiedOn] DATETIME NOT NULL
);
GO
