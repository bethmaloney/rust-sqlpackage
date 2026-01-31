CREATE NONCLUSTERED INDEX [IX_Table20_Name]
ON [dbo].[Table20] ([Name])
INCLUDE ([Description], [IsActive]);
GO
