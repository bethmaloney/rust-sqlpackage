-- View with ampersand in name (tests XML entity encoding)
CREATE VIEW [dbo].[Terms&ConditionsView]
AS
SELECT
    1 AS [Id],
    N'Standard Terms & Conditions' AS [Title],
    N'These are the standard terms & conditions for all orders.' AS [Content],
    GETDATE() AS [EffectiveDate];
GO
