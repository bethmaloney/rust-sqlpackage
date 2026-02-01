-- Instrument table for nested subquery alias tests
CREATE TABLE [dbo].[Instrument]
(
    [Id] INT NOT NULL PRIMARY KEY,
    [Name] NVARCHAR(100) NOT NULL,
    [AccountId] INT NULL
);
GO
