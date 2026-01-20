CREATE VIEW [dbo].[ActiveItems]
AS
SELECT [Id], [Name], [CreatedAt]
FROM [dbo].[BaseTable]
WHERE [IsActive] = 1;
