-- View with correlated subquery in SELECT list
-- Tests alias resolution in correlated subqueries
CREATE VIEW [dbo].[AccountWithCorrelatedSubquery]
AS
SELECT
    A.Id,
    A.AccountNumber,
    (
        SELECT COUNT(*)
        FROM [dbo].[AccountTag] [AT1]
        WHERE [AT1].AccountId = A.Id
    ) AS TagCount,
    (
        SELECT TOP 1 [T1].[Name]
        FROM [dbo].[AccountTag] [AT2]
        INNER JOIN [dbo].[Tag] [T1] ON [AT2].TagId = [T1].Id
        WHERE [AT2].AccountId = A.Id
        ORDER BY [T1].[Name]
    ) AS FirstTagName
FROM [dbo].[Account] A;
GO
