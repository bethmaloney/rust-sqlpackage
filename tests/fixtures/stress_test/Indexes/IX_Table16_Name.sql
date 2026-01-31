CREATE NONCLUSTERED INDEX [IX_Table16_Name]
ON [dbo].[Table16] ([Name])
INCLUDE ([Description], [IsActive]);
GO
