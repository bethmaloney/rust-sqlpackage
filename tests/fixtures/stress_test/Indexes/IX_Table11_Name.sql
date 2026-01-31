CREATE NONCLUSTERED INDEX [IX_Table11_Name]
ON [dbo].[Table11] ([Name])
INCLUDE ([Description], [IsActive]);
GO
