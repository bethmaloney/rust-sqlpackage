CREATE NONCLUSTERED INDEX [IX_Products_CategoryId]
ON [dbo].[Products] ([CategoryId])
INCLUDE ([Name], [Price], [IsActive]);
GO
