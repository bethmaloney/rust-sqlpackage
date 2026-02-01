-- View with CROSS APPLY and OUTER APPLY containing table aliases
-- Tests alias resolution in APPLY clauses
CREATE VIEW [dbo].[AccountWithApply]
AS
SELECT
    A.Id,
    A.AccountNumber,
    RecentTags.TagName,
    AllTags.TagCount
FROM [dbo].[Account] A
CROSS APPLY (
    -- Subquery with alias ATAG returning single tag
    SELECT TOP 1 [ATAG].[Name] AS TagName
    FROM [dbo].[AccountTag] [ATA]
    INNER JOIN [dbo].[Tag] [ATAG] ON [ATA].TagId = [ATAG].Id
    WHERE [ATA].AccountId = A.Id
    ORDER BY [ATAG].[Name]
) RecentTags
OUTER APPLY (
    -- Subquery with alias T3 returning count
    SELECT COUNT(*) AS TagCount
    FROM [dbo].[AccountTag] [AT3]
    INNER JOIN [dbo].[Tag] [T3] ON [AT3].TagId = [T3].Id
    WHERE [AT3].AccountId = A.Id
) AllTags;
GO
