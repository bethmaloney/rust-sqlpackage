CREATE NONCLUSTERED INDEX [IX_Table1_Name]
ON [dbo].[Table1] ([Name])
INCLUDE ([Description], [IsActive]);
GO
