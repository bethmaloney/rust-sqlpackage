-- View with table aliases that should NOT appear in BodyDependencies
-- The aliases A, T, AT should be resolved to actual tables, not treated as schema references
CREATE VIEW [dbo].[AccountSummary]
AS
SELECT
    A.Id,
    A.AccountNumber,
    A.Status,
    A.CreatedOn,
    T.Name AS TagName
FROM
    [dbo].[Account] A
    INNER JOIN [dbo].[AccountTag] AT ON AT.AccountId = A.Id
    INNER JOIN [dbo].[Tag] T ON T.Id = AT.TagId;
GO
