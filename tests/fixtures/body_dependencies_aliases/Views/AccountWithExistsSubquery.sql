-- View with EXISTS/NOT EXISTS containing table aliases
-- Tests alias resolution in EXISTS subqueries
CREATE VIEW [dbo].[AccountWithExistsSubquery]
AS
SELECT
    A.Id,
    A.AccountNumber
FROM [dbo].[Account] A
WHERE EXISTS (
    SELECT 1
    FROM [dbo].[AccountTag] [AT1]
    INNER JOIN [dbo].[Tag] [T1] ON [AT1].TagId = [T1].Id
    WHERE [AT1].AccountId = A.Id
        AND [T1].[Name] LIKE 'Important%'
)
AND NOT EXISTS (
    SELECT 1
    FROM [dbo].[AccountTag] [AT2]
    INNER JOIN [dbo].[Tag] [T2] ON [AT2].TagId = [T2].Id
    WHERE [AT2].AccountId = A.Id
        AND [T2].[Name] = 'Archived'
);
GO
