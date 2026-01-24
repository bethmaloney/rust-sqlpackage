-- Table for filtered index tests
CREATE TABLE [dbo].[Orders] (
    [Id] INT NOT NULL,
    [CustomerId] INT NOT NULL,
    [OrderDate] DATE NOT NULL,
    [TotalAmount] DECIMAL(18,2) NOT NULL,
    [Status] NVARCHAR(20) NOT NULL,
    [IsActive] BIT NOT NULL,
    [DeletedAt] DATETIME NULL,
    CONSTRAINT [PK_Orders] PRIMARY KEY ([Id])
);
GO
