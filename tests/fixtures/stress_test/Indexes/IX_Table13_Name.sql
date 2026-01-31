CREATE NONCLUSTERED INDEX [IX_Table13_Name]
ON [dbo].[Table13] ([Name])
INCLUDE ([Description], [IsActive]);
GO
