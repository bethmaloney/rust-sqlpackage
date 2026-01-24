-- Simple table for SQLCMD variable test
CREATE TABLE [dbo].[Settings] (
    [Id] INT NOT NULL PRIMARY KEY,
    [Key] NVARCHAR(100) NOT NULL,
    [Value] NVARCHAR(500) NULL
);
GO
