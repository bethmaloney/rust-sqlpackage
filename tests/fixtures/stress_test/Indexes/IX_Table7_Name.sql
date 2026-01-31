CREATE NONCLUSTERED INDEX [IX_Table7_Name]
ON [dbo].[Table7] ([Name])
INCLUDE ([Description], [IsActive]);
GO
