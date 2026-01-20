CREATE NONCLUSTERED INDEX [IX_Orders_CustomerId]
ON [Sales].[Orders] ([CustomerId])
INCLUDE ([OrderDate], [TotalAmount], [Status]);
GO
