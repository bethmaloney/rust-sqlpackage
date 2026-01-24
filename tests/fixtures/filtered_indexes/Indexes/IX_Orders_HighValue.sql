-- Filtered index on high-value orders (comparison filter)
CREATE NONCLUSTERED INDEX [IX_Orders_HighValue]
ON [dbo].[Orders] ([CustomerId])
INCLUDE ([TotalAmount], [OrderDate])
WHERE [TotalAmount] >= 1000.00;
GO
