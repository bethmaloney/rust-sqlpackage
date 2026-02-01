-- View with subquery inside CASE expression
-- Tests alias resolution in CASE subqueries
CREATE VIEW [dbo].[AccountWithCaseSubquery]
AS
SELECT
    A.Id,
    A.AccountNumber,
    CASE
        WHEN (
            SELECT COUNT(*)
            FROM [dbo].[AccountTag] [AT1]
            WHERE [AT1].AccountId = A.Id
        ) > 5 THEN 'High'
        WHEN (
            SELECT COUNT(*)
            FROM [dbo].[AccountTag] [AT2]
            WHERE [AT2].AccountId = A.Id
        ) > 0 THEN 'Low'
        ELSE 'None'
    END AS TagLevel,
    CASE
        WHEN EXISTS (
            SELECT 1
            FROM [dbo].[AccountTag] [AT3]
            INNER JOIN [dbo].[Tag] [T1] ON [AT3].TagId = [T1].Id
            WHERE [AT3].AccountId = A.Id AND [T1].[Name] = 'VIP'
        ) THEN 1
        ELSE 0
    END AS IsVip
FROM [dbo].[Account] A;
GO
