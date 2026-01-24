-- Table with composite PK (3 columns) - order header
CREATE TABLE [dbo].[OrderHeaders] (
    [Year] INT NOT NULL,
    [Region] CHAR(2) NOT NULL,
    [OrderNumber] INT NOT NULL,
    [CustomerId] INT NOT NULL,
    [OrderDate] DATE NOT NULL,
    [TotalAmount] DECIMAL(18,2) NOT NULL,
    CONSTRAINT [PK_OrderHeaders] PRIMARY KEY ([Year], [Region], [OrderNumber])
);
GO
