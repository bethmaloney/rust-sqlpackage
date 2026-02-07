CREATE TABLE [dbo].[Employees] (
    [Id] INT NOT NULL PRIMARY KEY,
    [Name] NVARCHAR(100) NOT NULL,
    [DepartmentId] INT NULL
);
GO

CREATE TABLE [dbo].[Departments] (
    [Id] INT NOT NULL PRIMARY KEY,
    [Name] NVARCHAR(100) NOT NULL
);
GO
