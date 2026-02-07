CREATE TABLE [dbo].[Orders]
(
    [OrderId] INT NOT NULL,
    [CustomerId] INT NOT NULL,
    [OrderDate] DATETIME2 NOT NULL,
    [TotalAmount] DECIMAL(18, 2) NOT NULL,
    [Status] NVARCHAR(20) NOT NULL,
    CONSTRAINT [PK_Orders] PRIMARY KEY CLUSTERED ([OrderId])
);
