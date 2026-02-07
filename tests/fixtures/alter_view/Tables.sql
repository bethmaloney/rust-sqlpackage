CREATE TABLE [dbo].[Users] (
    [Id] INT NOT NULL PRIMARY KEY,
    [Name] NVARCHAR(100) NOT NULL,
    [Email] NVARCHAR(255) NULL
);
GO

CREATE TABLE [dbo].[Orders] (
    [OrderId] INT NOT NULL PRIMARY KEY,
    [UserId] INT NOT NULL,
    [Total] DECIMAL(18,2) NOT NULL
);
