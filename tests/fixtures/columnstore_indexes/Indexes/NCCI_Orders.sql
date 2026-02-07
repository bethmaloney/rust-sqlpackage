CREATE NONCLUSTERED COLUMNSTORE INDEX [NCCI_Orders] ON [dbo].[Orders] ([OrderDate], [CustomerId], [TotalAmount]);
