CREATE PROCEDURE [dbo].[GetAccountWithCte]
    @AccountId INT
AS
BEGIN
    SET NOCOUNT ON;

    -- CTE example with single CTE
    WITH AccountCte AS (
        SELECT
            A.Id,
            A.AccountNumber,
            A.Status
        FROM [dbo].[Account] A
        WHERE A.Id = @AccountId
    )
    SELECT
        AccountCte.Id,
        AccountCte.AccountNumber,
        AccountCte.Status
    FROM AccountCte;

    -- CTE example with multiple CTEs
    WITH TagCte AS (
        SELECT T.Id, T.Name
        FROM [dbo].[Tag] T
    ),
    AccountTagCte AS (
        SELECT AT.AccountId, AT.TagId
        FROM [dbo].[AccountTag] AT
    )
    SELECT
        TagCte.Id AS TagId,
        TagCte.Name AS TagName,
        AccountTagCte.AccountId
    FROM TagCte
    INNER JOIN AccountTagCte ON AccountTagCte.TagId = TagCte.Id
    WHERE AccountTagCte.AccountId = @AccountId;
END;
GO
