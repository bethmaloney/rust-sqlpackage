-- View with multiple levels of nested subqueries and aliases
-- Tests alias resolution at different nesting depths
CREATE VIEW [dbo].[AccountWithNestedSubqueries]
AS
SELECT
    A.Id,
    A.AccountNumber,
    InnerData.TagCount,
    InnerData.FirstTagName
FROM [dbo].[Account] A
LEFT JOIN (
    -- First level subquery with alias AT1
    SELECT
        AT1.AccountId,
        COUNT(*) AS TagCount,
        (
            -- Second level nested subquery with alias T2
            SELECT TOP 1 [T2].[Name]
            FROM [dbo].[AccountTag] [AT2]
            INNER JOIN [dbo].[Tag] [T2] ON [AT2].TagId = [T2].Id
            WHERE [AT2].AccountId = AT1.AccountId
            ORDER BY [T2].[Name]
        ) AS FirstTagName
    FROM [dbo].[AccountTag] AT1
    GROUP BY AT1.AccountId
) InnerData ON InnerData.AccountId = A.Id;
GO
