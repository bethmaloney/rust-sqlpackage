-- View with UNION containing different aliases in each SELECT
-- Tests alias resolution across UNION branches
CREATE VIEW [dbo].[AccountWithUnion]
AS
SELECT
    A1.Id,
    A1.AccountNumber,
    T1.[Name] AS TagName,
    'Active' AS Source
FROM [dbo].[Account] A1
INNER JOIN [dbo].[AccountTag] AT1 ON AT1.AccountId = A1.Id
INNER JOIN [dbo].[Tag] T1 ON T1.Id = AT1.TagId
WHERE A1.Status = 'Active'

UNION ALL

SELECT
    A2.Id,
    A2.AccountNumber,
    T2.[Name] AS TagName,
    'Inactive' AS Source
FROM [dbo].[Account] A2
INNER JOIN [dbo].[AccountTag] AT2 ON AT2.AccountId = A2.Id
INNER JOIN [dbo].[Tag] T2 ON T2.Id = AT2.TagId
WHERE A2.Status = 'Inactive';
GO
