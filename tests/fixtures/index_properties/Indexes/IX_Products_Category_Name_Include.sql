-- Nonclustered index with INCLUDE columns
CREATE NONCLUSTERED INDEX [IX_Products_Category_Name_Include]
ON [dbo].[Products] ([Category], [Name])
INCLUDE ([Price], [Description]);
