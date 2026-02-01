-- View with IN clause containing subquery with aliases
-- Tests alias resolution in IN subqueries
CREATE VIEW [dbo].[AccountWithInSubquery]
AS
SELECT
    A.Id,
    A.AccountNumber,
    A.Status
FROM [dbo].[Account] A
WHERE A.Id IN (
    SELECT [AT1].AccountId
    FROM [dbo].[AccountTag] [AT1]
    INNER JOIN [dbo].[Tag] [T1] ON [AT1].TagId = [T1].Id
    WHERE [T1].[Name] IN (
        -- Nested IN subquery
        SELECT [T2].[Name]
        FROM [dbo].[Tag] [T2]
        WHERE [T2].Id > 0
    )
);
GO
