-- Unique nonclustered index on SKU
CREATE UNIQUE NONCLUSTERED INDEX [UX_Products_SKU]
ON [dbo].[Products] ([SKU]);
