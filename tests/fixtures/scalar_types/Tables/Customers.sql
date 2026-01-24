-- Table using user-defined scalar types
CREATE TABLE [dbo].[Customers] (
    [Id] INT NOT NULL,
    [Name] NVARCHAR(100) NOT NULL,
    [Phone] [dbo].[PhoneNumber],
    [Email] [dbo].[EmailAddress],
    [CreditLimit] [dbo].[Currency],
    [TaxId] [dbo].[SSN],
    CONSTRAINT [PK_Customers] PRIMARY KEY ([Id])
);
GO
