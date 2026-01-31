CREATE NONCLUSTERED INDEX [IX_Table12_Name]
ON [dbo].[Table12] ([Name])
INCLUDE ([Description], [IsActive]);
GO
