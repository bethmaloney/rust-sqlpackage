CREATE NONCLUSTERED INDEX [IX_Table2_Name]
ON [dbo].[Table2] ([Name])
INCLUDE ([Description], [IsActive]);
GO
