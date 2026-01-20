CREATE TABLE [Sales].[Customers] (
    [Id] INT NOT NULL IDENTITY(1, 1),
    [FirstName] NVARCHAR(100) NOT NULL,
    [LastName] NVARCHAR(100) NOT NULL,
    [Email] NVARCHAR(255) NOT NULL,
    [Phone] NVARCHAR(20) NULL,
    [CreatedAt] DATETIME NOT NULL,

    CONSTRAINT [PK_Customers] PRIMARY KEY CLUSTERED ([Id]),
    CONSTRAINT [UQ_Customers_Email] UNIQUE ([Email]),
    CONSTRAINT [DF_Customers_CreatedAt] DEFAULT (GETDATE()) FOR [CreatedAt]
);
GO
