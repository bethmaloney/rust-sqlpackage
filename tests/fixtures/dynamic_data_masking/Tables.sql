CREATE TABLE [dbo].[Customers] (
    [CustomerId] INT NOT NULL PRIMARY KEY,
    [FirstName] NVARCHAR(50) NOT NULL,
    [LastName] NVARCHAR(50) NOT NULL,
    [Email] NVARCHAR(100) MASKED WITH (FUNCTION = 'email()') NULL,
    [SSN] CHAR(11) MASKED WITH (FUNCTION = 'partial(0,"XXX-XX-",4)') NULL,
    [CreditCard] VARCHAR(20) MASKED WITH (FUNCTION = 'default()') NULL,
    [Salary] DECIMAL(18, 2) MASKED WITH (FUNCTION = 'random(1000, 50000)') NULL,
    [Phone] VARCHAR(20) MASKED WITH (FUNCTION = 'partial(1,"XXXXXXX",0)') NOT NULL
);
GO

CREATE TABLE [dbo].[Orders] (
    [OrderId] INT NOT NULL PRIMARY KEY,
    [CustomerId] INT NOT NULL,
    [OrderDate] DATETIME NOT NULL,
    [Total] DECIMAL(18, 2) NOT NULL
);
GO
